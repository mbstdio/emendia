//! Opt-in tests on a disposable X11 desktop with a real GTK editor in a child process.
use super::*;
use gtk::prelude::*;
use std::process::{Child, Command};

const FIXTURE: &str = "platform::linux::x11::desktop_tests::edit_fixture";

struct ChildProcess(Child);
impl Drop for ChildProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait(check: impl Fn() -> Result<bool>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !check()? {
        if Instant::now() >= deadline {
            bail!("X11 fixture timed out");
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn command(desktop: &Desktop, values: &[u32]) -> Result<()> {
    desktop
        .connection
        .change_property32(
            PropMode::REPLACE,
            desktop.root,
            atom(&desktop.connection, "_EMENDIA_TEST_COMMAND")?,
            AtomEnum::CARDINAL,
            values,
        )?
        .check()?;
    wait(|| {
        Ok(desktop
            .values(desktop.root, "_EMENDIA_TEST_COMMAND")?
            .is_empty())
    })
}

fn document(desktop: &Desktop) -> Result<String> {
    let text = desktop
        .connection
        .get_property(
            false,
            desktop.root,
            atom(&desktop.connection, "_EMENDIA_TEST_TEXT")?,
            AtomEnum::ANY,
            0,
            100_000,
        )?
        .reply()?
        .value;
    Ok(String::from_utf8(text)?)
}

fn icon_window(desktop: &Desktop) -> Result<Window> {
    let window = desktop.connection.generate_id()?;
    desktop
        .connection
        .create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            desktop.root,
            0,
            0,
            64,
            64,
            0,
            xproto::WindowClass::INPUT_OUTPUT,
            0,
            &xproto::CreateWindowAux::new(),
        )?
        .check()?;
    set_window_icon(window as isize)?;
    Ok(window)
}

#[test]
#[ignore = "requires a disposable X11 desktop and D-Bus session"]
fn x11_tray_and_global_shortcuts() -> Result<()> {
    use dbus::{
        blocking::stdintf::org_freedesktop_dbus::Properties,
        channel::{MatchingReceiver, Sender},
        message::MatchRule,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    // A minimal StatusNotifierWatcher verifies that GTK registers a real tray
    // item. Other graphical tests exercise desktops without an indicator host.
    let watcher = dbus::blocking::Connection::new_session()?;
    watcher.request_name("org.kde.StatusNotifierWatcher", false, true, false)?;
    let registered = Arc::new(AtomicBool::new(false));
    let registration = Arc::new(std::sync::Mutex::new(None::<(String, String)>));
    let flag = registered.clone();
    let item = registration.clone();
    watcher.start_receive(
        MatchRule::new_method_call(),
        Box::new(move |message, connection| {
            let method = message
                .member()
                .map(|value| value.to_string())
                .unwrap_or_default();
            let reply = match method.as_str() {
                "RegisterStatusNotifierItem" => {
                    let name: String = message.read1().expect("StatusNotifierItem registration");
                    let (service, path) = if name.starts_with('/') {
                        (message.sender().unwrap().to_string(), name)
                    } else {
                        (name, "/StatusNotifierItem".to_owned())
                    };
                    *item.lock().unwrap() = Some((service, path));
                    flag.store(true, Ordering::Relaxed);
                    message.method_return()
                }
                "GetAll" => {
                    let mut properties = dbus::arg::PropMap::new();
                    properties.insert(
                        "IsStatusNotifierHostRegistered".into(),
                        dbus::arg::Variant(Box::new(true)),
                    );
                    properties.insert(
                        "ProtocolVersion".into(),
                        dbus::arg::Variant(Box::new(0_i32)),
                    );
                    message.method_return().append1(properties)
                }
                "Get" => message.method_return().append1(dbus::arg::Variant(true)),
                _ => message.method_return(),
            };
            let _ = connection.send(reply);
            true
        }),
    );
    let mut tray = crate::platform::tray::Tray::new(false)?;
    assert!(tray.available(), "GTK tray initialization must succeed");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !registered.load(Ordering::Relaxed) {
        watcher.process(Duration::from_millis(20))?;
        if Instant::now() >= deadline {
            bail!("Tray did not register with StatusNotifierWatcher");
        }
    }
    let (service, path) = registration.lock().unwrap().clone().unwrap();
    let proxy = watcher.with_proxy(service, path, Duration::from_millis(500));
    let icon_path = || -> Result<std::path::PathBuf> {
        let name: String = proxy.get("org.kde.StatusNotifierItem", "IconName")?;
        let path = std::path::PathBuf::from(name);
        if path.is_absolute() {
            return Ok(path);
        }
        let directory: String = proxy.get("org.kde.StatusNotifierItem", "IconThemePath")?;
        Ok(std::path::PathBuf::from(directory).join(path))
    };
    let check_variant = |dark: bool| -> Result<std::path::PathBuf> {
        let source: &[u8] = if dark {
            include_bytes!("../../ressources/app-logo-dark.png")
        } else {
            include_bytes!("../../ressources/app-logo-light.png")
        };
        let image = image::load_from_memory(source)?.into_rgba8();
        let expected =
            image::imageops::resize(&image, 64, 64, image::imageops::FilterType::Lanczos3);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(path) = icon_path()
                && let Ok(actual) = image::open(&path)
                && actual.into_rgba8() == expected
            {
                return Ok(path);
            }
            if Instant::now() >= deadline {
                bail!("Tray did not publish the expected system-theme logo");
            }
            thread::sleep(Duration::from_millis(20));
        }
    };
    let light_path = check_variant(false)?;
    let desktop = Desktop::new()?;
    let app_window = icon_window(&desktop)?;
    assert!(smoke_window_has_icon(app_window as isize));
    tray.set_system_theme(true)?;
    let dark_path = check_variant(true)?;
    assert!(
        smoke_window_has_icon(app_window as isize),
        "A dark tray must not change the fixed light window logo"
    );
    assert_ne!(
        light_path, dark_path,
        "Theme changes must replace the native indicator icon"
    );
    tray.set_system_theme(true)?;
    thread::sleep(Duration::from_millis(100));
    assert_eq!(
        icon_path()?,
        dark_path,
        "An unchanged theme must not rewrite the icon"
    );
    tray.set_system_theme(false)?;
    assert_ne!(
        check_variant(false)?,
        dark_path,
        "Switching back must restore the light variant"
    );
    assert!(smoke_window_has_icon(app_window as isize));
    desktop.connection.destroy_window(app_window)?.check()?;
    tray.set_enabled(false);
    tray.localize();
    drop(tray);

    let mut hotkeys = crate::platform::hotkey::HotkeyRegistration::new()?;
    hotkeys.change(crate::settings::Settings::default().shortcuts())?;
    let competing = global_hotkey::GlobalHotKeyManager::new()?;
    assert!(
        competing
            .register(crate::platform::hotkey::parse("Ctrl+F12")?)
            .is_err()
    );
    desktop.send_ctrl(0xffc9)?; // F12
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(event) = crate::platform::hotkey::next_event()
            && event.state == global_hotkey::HotKeyState::Pressed
        {
            assert_eq!(
                hotkeys.mode(event.id),
                Some(crate::platform::hotkey::TranslationMode::Preview)
            );
            break;
        }
        if Instant::now() >= deadline {
            bail!("The registered X11 hotkey did not fire");
        }
        thread::sleep(Duration::from_millis(10));
    }
    // Reassign an already-registered combination between all four actions. The
    // OS reservation is retained, but event dispatch must follow each saved map.
    let mut values = [
        "Ctrl+KeyM".to_owned(),
        "Ctrl+Shift+F12".to_owned(),
        "Ctrl+F11".to_owned(),
        "Ctrl+Shift+F11".to_owned(),
    ];
    hotkeys.change(values.each_ref().map(String::as_str))?;
    for target in crate::platform::hotkey::TranslationMode::ALL
        .into_iter()
        .cycle()
        .take(12)
    {
        let source = values
            .iter()
            .position(|value| value == "Ctrl+KeyM")
            .unwrap();
        values.swap(source, target.index());
        hotkeys.change(values.each_ref().map(String::as_str))?;
        desktop.send_ctrl(b'm' as u32)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(event) = crate::platform::hotkey::next_event()
                && event.state == global_hotkey::HotKeyState::Pressed
            {
                assert_eq!(hotkeys.mode(event.id), Some(target));
                break;
            }
            if Instant::now() >= deadline {
                bail!("Reassigned Ctrl+M did not fire");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    hotkeys.disable()?;
    let released = crate::platform::hotkey::parse("Ctrl+KeyM")?;
    competing.register(released)?;
    competing.unregister(released)?;
    Ok(())
}

#[test]
#[ignore = "requires a disposable X11 desktop"]
fn x11_taskbar_icon_is_embedded_without_an_installed_launcher() -> Result<()> {
    let desktop = Desktop::new()?;
    let window = icon_window(&desktop)?;
    assert!(smoke_window_has_icon(window as isize));
    // Re-applying must replace the complete multi-size property, never append
    // duplicate icons or change the fixed base logo according to the tray theme.
    set_window_icon(window as isize)?;
    assert!(smoke_window_has_icon(window as isize));
    desktop.connection.destroy_window(window)?.check()?;
    Ok(())
}

#[test]
#[ignore = "requires a disposable D-Bus session with an unlocked Secret Service"]
fn linux_keyring_persists_across_processes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let endpoint = format!(
        "https://emendia-test.invalid/{}",
        directory.path().file_name().unwrap().to_string_lossy()
    );
    crate::settings::save_api_key(&endpoint, "emendia-keyring-test")?;
    let result = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "platform::linux::x11::desktop_tests::keyring_fixture",
            "--ignored",
            "--nocapture",
        ])
        .env("EMENDIA_KEYRING_TEST_ENDPOINT", &endpoint)
        .status();
    crate::settings::save_api_key(&endpoint, "")?;
    assert!(
        result?.success(),
        "Credentials must be readable from another process"
    );
    assert!(crate::settings::load_api_key(&endpoint)?.is_empty());
    Ok(())
}

#[test]
#[ignore = "helper process for linux_keyring_persists_across_processes"]
fn keyring_fixture() -> Result<()> {
    if let Ok(endpoint) = std::env::var("EMENDIA_KEYRING_TEST_ENDPOINT") {
        assert_eq!(
            crate::settings::load_api_key(&endpoint)?,
            "emendia-keyring-test"
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires a disposable X11 desktop; temporarily uses focus and clipboard"]
fn x11_clipboard_and_native_edit_round_trip() -> Result<()> {
    let desktop = Desktop::new()?;
    let mut reader = Reader::new()?;
    copy_text("presse-papiers initial")?;
    let (_, initial) = reader
        .snapshot()
        .context("Snapshot the initial clipboard")?;
    let old = clipboard::put_text("ancienne copie")?;
    clipboard::put_text("nouvelle copie utilisateur")?;
    assert!(
        clipboard::put(initial, Some(old)).is_err(),
        "A concurrent writer must win"
    );
    assert_eq!(
        reader
            .text()
            .context("Read the concurrent writer's clipboard")?,
        "nouvelle copie utilisateur"
    );

    // Exercise incoming and outgoing INCR transfers, preserving rich and binary formats.
    let text = "é".repeat(40_000);
    copy_text(&text)?;
    let (_, mut saved) = reader.snapshot().context("Snapshot the large clipboard")?;
    let html = atom(&reader.connection, "text/html")?;
    let binary = atom(&reader.connection, "image/png")?;
    let rich = b"<b>clipboard</b>".to_vec();
    let image = vec![0, 1, 255, 128, 0];
    saved.push(clipboard::Format {
        target: html,
        type_: html,
        format: 8,
        data: rich.clone(),
    });
    saved.push(clipboard::Format {
        target: binary,
        type_: binary,
        format: 8,
        data: image.clone(),
    });
    let temporary = clipboard::put_text("temporaire")?;
    clipboard::put(saved, Some(temporary))?;
    assert_eq!(
        reader.text().context("Read the restored large clipboard")?,
        text
    );
    let (_, restored) = reader
        .snapshot()
        .context("Snapshot restored rich formats")?;
    assert!(
        restored
            .iter()
            .any(|value| value.target == html && value.data == rich)
    );
    assert!(
        restored
            .iter()
            .any(|value| value.target == binary && value.data == image)
    );
    copy_text("nouvelle copie utilisateur")?;

    let child = Command::new(std::env::current_exe()?)
        .args(["--exact", FIXTURE, "--ignored", "--nocapture"])
        .env("EMENDIA_X11_EDIT_FIXTURE", "1")
        .spawn()?;
    let pid = child.id();
    let _child = ChildProcess(child);
    wait(|| {
        Ok(desktop.active().ok().is_some_and(|window| {
            desktop
                .destination(window)
                .is_ok_and(|destination| destination.process == Some(pid))
        }))
    })?;
    // The WM can publish _NET_ACTIVE_WINDOW before GTK has established keyboard
    // focus in its TextView. Wait for the fixture's event loop and initial focus
    // transition, rather than treating that transition as a changed destination.
    wait(|| Ok(document(&desktop)? == "Avant Bonjour Après"))?;
    thread::sleep(Duration::from_millis(200));
    let target = capture_target()?;
    let fixture = target.destination.window;
    let selection = capture(target).context("initial X11 capture")?;
    assert_eq!(selection.text, "Bonjour");
    assert_eq!(
        reader.text()?,
        "nouvelle copie utilisateur",
        "Capture must restore the previous clipboard"
    );
    replace(&selection, "Hello").context("X11 replacement")?;
    wait(|| Ok(document(&desktop)? == "Avant Hello Après"))?;
    assert_eq!(reader.text()?, "Hello");

    command(&desktop, &[1, 6, 11])?;
    let target = capture_target()?;
    let placement = status_placement(&target)?;
    let status = desktop.connection.generate_id()?;
    desktop
        .connection
        .create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            status,
            desktop.root,
            0,
            0,
            340,
            160,
            0,
            xproto::WindowClass::INPUT_OUTPUT,
            0,
            &xproto::CreateWindowAux::new(),
        )?
        .check()?;
    show_status_without_activation(status as isize, placement, 1.)?;
    assert!(status_is_nonactivating(status as isize));
    assert!(status_has_expected_bounds(status as isize, placement, 1.));
    assert_eq!(desktop.active()?, fixture, "Status must not steal focus");
    let quick = capture(target)?;
    assert_eq!(quick.text, "Hello");
    replace_quick(&quick, "Salut")?;
    wait(|| Ok(document(&desktop)? == "Avant Salut Après"))?;
    assert_eq!(desktop.active()?, fixture);
    desktop.connection.destroy_window(status)?.check()?;

    command(&desktop, &[1, 0, 5])?;
    assert!(
        replace_quick(&quick, "wrong").is_err(),
        "Changed text must prevent replacement"
    );
    assert_eq!(reader.text()?, "Salut");
    command(&desktop, &[1, 6, 11])?;
    let original = capture(capture_target()?)?;
    command(&desktop, &[3])?;
    wait(|| Ok(desktop.active().is_ok_and(|window| window != fixture)))?;
    let other = desktop.active()?;
    assert!(replace_quick(&original, "wrong").is_err());
    assert_eq!(
        desktop.active()?,
        other,
        "Quick must not reactivate the original window"
    );
    replace(&original, "Bonjour")?;
    wait(|| Ok(document(&desktop)? == "Avant Bonjour Après"))?;

    command(&desktop, &[2, 70_000])?;
    let large = capture(capture_target()?)?;
    assert_eq!(
        large.text.len(),
        70_000,
        "Native GTK INCR capture must complete"
    );
    command(&desktop, &[2, 100_001])?;
    assert!(capture(capture_target()?).is_err());
    assert_eq!(
        reader.text()?,
        "Bonjour",
        "Oversized captures must still restore clipboard data"
    );
    Ok(())
}

#[test]
#[ignore = "helper process for x11_clipboard_and_native_edit_round_trip"]
fn edit_fixture() -> Result<()> {
    if std::env::var_os("EMENDIA_X11_EDIT_FIXTURE").is_none() {
        return Ok(());
    }
    gtk::init()?;
    let desktop = Desktop::new()?;
    let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    buffer.set_text("Avant Bonjour Après");
    let entry = gtk::TextView::with_buffer(&buffer);
    entry.set_wrap_mode(gtk::WrapMode::Char);
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Emendia X11 test fixture");
    window.set_default_size(600, 100);
    window.add(&entry);
    window.show_all();
    window.present();
    entry.grab_focus();
    buffer.select_range(&buffer.iter_at_offset(6), &buffer.iter_at_offset(13));
    let other = gtk::Window::new(gtk::WindowType::Toplevel);
    other.set_title("Emendia X11 alternate window");
    other.set_default_size(300, 100);
    let command_atom = atom(&desktop.connection, "_EMENDIA_TEST_COMMAND")?;
    let text_atom = atom(&desktop.connection, "_EMENDIA_TEST_TEXT")?;
    desktop
        .connection
        .delete_property(desktop.root, command_atom)?
        .check()?;
    gtk::glib::timeout_add_local(Duration::from_millis(20), move || {
        let result = (|| -> Result<()> {
            let command = desktop.values(desktop.root, "_EMENDIA_TEST_COMMAND")?;
            match command.first() {
                Some(1) => buffer.select_range(
                    &buffer.iter_at_offset(command[1] as i32),
                    &buffer.iter_at_offset(command[2] as i32),
                ),
                Some(2) => {
                    buffer.set_text(&"x".repeat(command[1] as usize));
                    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
                }
                Some(3) => {
                    other.show_all();
                    other.present();
                }
                _ => {}
            }
            desktop
                .connection
                .change_property8(
                    PropMode::REPLACE,
                    desktop.root,
                    text_atom,
                    atom(&desktop.connection, "UTF8_STRING")?,
                    buffer
                        .text(&buffer.start_iter(), &buffer.end_iter(), true)
                        .unwrap()
                        .as_bytes(),
                )?
                .check()?;
            if !command.is_empty() {
                desktop
                    .connection
                    .delete_property(desktop.root, command_atom)?
                    .check()?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            panic!("GTK fixture failed: {error:#}");
        }
        gtk::glib::ControlFlow::Continue
    });
    gtk::main();
    Ok(())
}
