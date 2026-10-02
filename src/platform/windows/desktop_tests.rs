//! Opt-in tests for a real interactive Windows session. A separate process hosts
//! a native EDIT control, so focus, clipboard ownership and SendInput are genuine.
use super::*;
use std::process::{Child, Command};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    Graphics::Gdi::CreateBitmap,
    System::Threading::{AttachThreadInput, GetCurrentThreadId},
    UI::{Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
};

const FIXTURE: &str = "platform::windows::desktop_tests::edit_fixture";

struct ChildProcess(Child);
impl Drop for ChildProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct RestoreClipboard(Option<ClipboardSnapshot>);
impl Drop for RestoreClipboard {
    fn drop(&mut self) {
        if let Some(snapshot) = self.0.take() {
            // Tests deliberately own the clipboard for their duration; restore user data
            // at the end. Do not run these while using another clipboard application.
            let _ = snapshot.restore_if_unchanged(unsafe { GetClipboardSequenceNumber() });
        }
    }
}

#[test]
#[ignore = "requires an interactive Windows desktop and temporarily uses focus/clipboard"]
fn clipboard_and_native_edit_round_trip() -> Result<()> {
    let _restore = RestoreClipboard(Some(ClipboardSnapshot::take()?));

    // Rich clipboard restoration, including the GDI path OleDuplicateData handles.
    let sequence = write_text("presse-papiers initial")?;
    {
        let _lock = ClipboardLock::acquire()?;
        assert_eq!(unsafe { GetClipboardSequenceNumber() }, sequence);
        unsafe {
            let bitmap = CreateBitmap(2, 2, 1, 32, None);
            assert!(!bitmap.0.is_null());
            SetClipboardData(2, HANDLE(bitmap.0))?;
        }
    }
    let rich = ClipboardSnapshot::take()?;
    let sequence = write_text("temporaire")?;
    rich.restore_if_unchanged(sequence)?;
    assert_eq!(
        read_text(unsafe { GetCurrentProcessId() })?.0,
        "presse-papiers initial"
    );
    {
        let _lock = ClipboardLock::acquire()?;
        assert!(!unsafe { GetClipboardData(2)? }.0.is_null());
    }

    // A new writer must win over an older snapshot's restoration.
    let old = ClipboardSnapshot::take()?;
    let old_sequence = write_text("ancienne copie")?;
    write_text("nouvelle copie utilisateur")?;
    old.restore_if_unchanged(old_sequence)?;
    assert_eq!(
        read_text(unsafe { GetCurrentProcessId() })?.0,
        "nouvelle copie utilisateur"
    );

    let child = Command::new(std::env::current_exe()?)
        .args(["--exact", FIXTURE, "--ignored", "--nocapture"])
        .env("TRANSLATION_TOOL_EDIT_FIXTURE", "1")
        .spawn()?;
    let _child = ChildProcess(child);
    let deadline = Instant::now() + Duration::from_secs(5);
    let fixture = loop {
        if let Ok(window) = unsafe { FindWindowW(None, w!("Emendia native test fixture")) } {
            break window;
        }
        if Instant::now() >= deadline {
            bail!("Native fixture failed to open");
        }
        thread::sleep(Duration::from_millis(30));
    };
    let edit = loop {
        if let Ok(edit) = unsafe { FindWindowExW(fixture, None, w!("EDIT"), None) } {
            break edit;
        }
        if Instant::now() >= deadline {
            bail!("Native edit fixture failed to open");
        }
        thread::sleep(Duration::from_millis(20));
    };
    thread::sleep(Duration::from_millis(200));
    assert_eq!(
        unsafe { GetForegroundWindow() },
        fixture,
        "Fixture must own foreground"
    );
    let selected = unsafe { SendMessageW(edit, 0x00b0, WPARAM(0), LPARAM(0)) }.0 as u32;
    assert_eq!(
        (selected & 0xffff, selected >> 16),
        (6, 13),
        "Fixture must have a selected range"
    );
    assert_eq!(
        destination(fixture.0 as isize)?.focused_control,
        edit.0 as isize,
        "Edit must own keyboard focus"
    );
    let selection =
        capture(capture_target().context("initial capture target")?).context("initial capture")?;
    assert_eq!(selection.text, "Bonjour");
    assert_eq!(
        read_text(unsafe { GetCurrentProcessId() })?.0,
        "nouvelle copie utilisateur"
    );
    replace(&selection, "Hello").context("replace native edit selection")?;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let mut text = [0u16; 128];
        let length = unsafe {
            SendMessageW(
                edit,
                WM_GETTEXT,
                WPARAM(text.len()),
                LPARAM(text.as_mut_ptr() as isize),
            )
        }
        .0 as usize;
        if String::from_utf16_lossy(&text[..length]) == "Avant Hello Après" {
            break;
        }
        if Instant::now() >= deadline {
            bail!(
                "Native edit did not receive translated paste; got {:?}",
                String::from_utf16_lossy(&text[..length])
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(read_text(unsafe { GetCurrentProcessId() })?.0, "Hello");
    // The background path must paste without foreground activation as well.
    unsafe {
        SendMessageW(edit, 0x00b1, WPARAM(6), LPARAM(11));
    }
    // Keep a non-activating status popup visible through both capture and paste.
    let target = capture_target()?;
    let status_position = status_placement(&target)?;
    let status = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("Correction en cours…"),
            WS_POPUP,
            status_position.x as i32,
            status_position.y as i32,
            340,
            160,
            None,
            None,
            None,
            None,
        )?
    };
    let status_owner = ClipboardOwner(status);
    show_status_without_activation(status.0 as isize, status_position)?;
    assert!(status_has_expected_bounds(
        status.0 as isize,
        status_position
    ));
    assert!(status_is_nonactivating(status.0 as isize));
    assert_eq!(unsafe { GetForegroundWindow() }, fixture);
    assert_eq!(
        destination(fixture.0 as isize)?.focused_control,
        edit.0 as isize
    );
    let quick_selection = capture(target)?;
    assert_eq!(quick_selection.text, "Hello");
    replace_quick(&quick_selection, "Salut")?;
    assert_eq!(unsafe { GetForegroundWindow() }, fixture);
    drop(status_owner);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let mut text = [0u16; 128];
        let length = unsafe {
            SendMessageW(
                edit,
                WM_GETTEXT,
                WPARAM(text.len()),
                LPARAM(text.as_mut_ptr() as isize),
            )
        }
        .0 as usize;
        if String::from_utf16_lossy(&text[..length]) == "Avant Salut Après" {
            break;
        }
        if Instant::now() >= deadline {
            bail!("Quick Translate did not paste in the native edit");
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(read_text(unsafe { GetCurrentProcessId() })?.0, "Salut");
    // A stale selection must not trigger a background paste or change the clipboard.
    assert!(replace_quick(&quick_selection, "wrong").is_err());
    assert_eq!(read_text(unsafe { GetCurrentProcessId() })?.0, "Salut");

    // A different foreground window must be left alone by the quick path.
    unsafe {
        SendMessageW(edit, 0x00b1, WPARAM(6), LPARAM(11));
    }
    let quick_selection = capture(capture_target()?)?;
    let other = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW,
            w!("STATIC"),
            w!("Quick Translate focus test"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            150,
            150,
            200,
            100,
            None,
            None,
            None,
            None,
        )?
    };
    unsafe {
        let _ = SetForegroundWindow(other);
    }
    assert_eq!(unsafe { GetForegroundWindow() }, other);
    let result = replace_quick(&quick_selection, "wrong");
    let foreground = unsafe { GetForegroundWindow() };
    unsafe {
        // STATIC does not restore focus to its EDIT child like a real editor does.
        let fixture_thread = GetWindowThreadProcessId(fixture, None);
        let test_thread = GetCurrentThreadId();
        let attached = AttachThreadInput(test_thread, fixture_thread, true).as_bool();
        let _ = SetForegroundWindow(fixture);
        let _ = SetFocus(edit);
        if attached {
            let _ = AttachThreadInput(test_thread, fixture_thread, false);
        }
        DestroyWindow(other)?;
    }
    assert!(result.is_err());
    assert_eq!(foreground, other, "Quick Translate must not steal focus");
    // Identical text at a different offset must not count as the original selection.
    unsafe {
        assert_ne!(
            SendMessageW(
                edit,
                WM_SETTEXT,
                WPARAM(0),
                LPARAM(w!("Bonjour Bonjour").as_ptr() as isize)
            )
            .0,
            0
        );
        SendMessageW(edit, 0x00b1, WPARAM(0), LPARAM(7)); // EM_SETSEL
    }
    let original = capture(capture_target()?).context("capture after restoring fixture focus")?;
    assert_eq!(original.text, "Bonjour");
    unsafe {
        SendMessageW(edit, 0x00b1, WPARAM(8), LPARAM(15));
    }
    if original
        .accessibility
        .as_ref()
        .is_some_and(|identity| identity.prefix_units.is_some())
    {
        assert!(
            replace(&original, "wrong").is_err(),
            "A different range of identical text must be rejected"
        );
    }
    // Rejected oversized captures must still restore the user's clipboard.
    write_text("avant une capture trop longue")?;
    let oversized: Vec<u16> = "x"
        .repeat(MAX_TEXT_UNITS + 1)
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        SendMessageW(edit, 0x00c5, WPARAM(MAX_TEXT_UNITS + 100), LPARAM(0)); // EM_SETLIMITTEXT
        assert_ne!(
            SendMessageW(
                edit,
                WM_SETTEXT,
                WPARAM(0),
                LPARAM(oversized.as_ptr() as isize)
            )
            .0,
            0
        );
        SendMessageW(
            edit,
            0x00b1,
            WPARAM(0),
            LPARAM((MAX_TEXT_UNITS + 1) as isize),
        );
    }
    let error = capture(capture_target()?).unwrap_err();
    assert!(error.to_string().contains("100 000"));
    assert_eq!(
        read_text(unsafe { GetCurrentProcessId() })?.0,
        "avant une capture trop longue"
    );
    unsafe {
        PostMessageW(fixture, WM_CLOSE, WPARAM(0), LPARAM(0))?;
    }
    Ok(())
}

#[test]
#[ignore = "helper process for clipboard_and_native_edit_round_trip"]
fn edit_fixture() -> Result<()> {
    if std::env::var_os("TRANSLATION_TOOL_EDIT_FIXTURE").is_none() {
        return Ok(());
    }
    unsafe {
        let fixture = CreateWindowExW(
            WS_EX_APPWINDOW,
            w!("STATIC"),
            w!("Emendia native test fixture"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100,
            100,
            600,
            200,
            None,
            None,
            None,
            None,
        )?;
        let edit = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("EDIT"),
            w!("Avant Bonjour Après"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE((ES_MULTILINE | ES_NOHIDESEL) as u32),
            20,
            30,
            550,
            50,
            fixture,
            None,
            None,
            None,
        )?;
        // The test harness is launched in the background by automation, without a
        // user's foreground activation. Attach only for fixture activation, then
        // detach; production replacement does not bypass Windows focus rules.
        let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let test_thread = GetCurrentThreadId();
        let attached = AttachThreadInput(test_thread, foreground_thread, true).as_bool();
        let _ = SetForegroundWindow(fixture);
        // SetFocus returns the PREVIOUS focus HWND; null is valid on first focus.
        let _ = SetFocus(edit);
        if attached {
            let _ = AttachThreadInput(test_thread, foreground_thread, false);
        }
        SendMessageW(edit, 0x00b1, WPARAM(6), LPARAM(13));
        let mut message = MSG::default();
        while IsWindow(fixture).as_bool() {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
    Ok(())
}
