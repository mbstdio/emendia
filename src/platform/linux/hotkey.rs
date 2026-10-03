//! X11 grabs with checked release, keeping the shared HotKey parser and IDs.
//! Acknowledgements follow server round trips: released keys must immediately
//! become available to GPUI's recorder and other clients, including the last key.
use anyhow::{Context, Result, bail};
use global_hotkey::{
    GlobalHotKeyEvent, HotKeyState,
    hotkey::{HotKey, Modifiers},
};
use std::{
    sync::{Mutex, OnceLock, mpsc},
    thread,
    time::Duration,
};
use x11rb::{
    connection::Connection,
    protocol::{
        Event, xkb,
        xproto::{ConnectionExt as _, GrabMode, KeyButMask, ModMask},
    },
    rust_connection::RustConnection,
};

type Reply = mpsc::SyncSender<Result<()>>;
enum Command {
    Register(HotKey, Reply),
    Unregister(HotKey, Reply),
    Quit,
}

type Events = (
    mpsc::Sender<GlobalHotKeyEvent>,
    Mutex<mpsc::Receiver<GlobalHotKeyEvent>>,
);
static EVENTS: OnceLock<Events> = OnceLock::new();

fn events() -> &'static Events {
    EVENTS.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        (sender, Mutex::new(receiver))
    })
}

pub(crate) fn next_event() -> Option<GlobalHotKeyEvent> {
    events().1.lock().ok()?.try_recv().ok()
}

pub(crate) struct Manager {
    commands: mpsc::Sender<Command>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Manager {
    pub fn new() -> Result<Self> {
        let (commands, receiver) = mpsc::channel();
        let (ready, initialized) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("emendia-hotkeys".into())
            .spawn(move || {
                let service = Service::new();
                let mut service = match service {
                    Ok(service) => {
                        let _ = ready.send(Ok(()));
                        service
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                if let Err(error) = service.run(receiver) {
                    tracing::error!(%error, "X11 shortcut service stopped");
                }
            })?;
        initialized
            .recv()
            .context("X11 shortcut initialization interrupted")??;
        Ok(Self {
            commands,
            thread: Some(thread),
        })
    }

    pub fn register(&self, key: HotKey) -> Result<()> {
        self.request(|reply| Command::Register(key, reply))
    }
    pub fn unregister(&self, key: HotKey) -> Result<()> {
        self.request(|reply| Command::Unregister(key, reply))
    }

    fn request(&self, command: impl FnOnce(Reply) -> Command) -> Result<()> {
        let (reply, result) = mpsc::sync_channel(1);
        self.commands
            .send(command(reply))
            .context("X11 shortcut service unavailable")?;
        result.recv().context("X11 shortcut service interrupted")?
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Binding {
    key: HotKey,
    code: u8,
    modifiers: ModMask,
    pressed: bool,
}
struct Service {
    connection: RustConnection,
    root: u32,
    locks: Vec<ModMask>,
    bindings: Vec<Binding>,
}

impl Service {
    fn new() -> Result<Self> {
        let (connection, screen) = x11rb::connect(None)?;
        let root = connection.setup().roots[screen].root;
        xkb::ConnectionExt::xkb_use_extension(&connection, 1, 0)?.reply()?;
        xkb::ConnectionExt::xkb_per_client_flags(
            &connection,
            xkb::ID::USE_CORE_KBD.into(),
            xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
            xkb::PerClientFlag::DETECTABLE_AUTO_REPEAT,
            Default::default(),
            Default::default(),
            Default::default(),
        )?
        .reply()?;
        let setup = connection.setup();
        let mapping = connection
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)?
            .reply()?;
        let modifiers = connection.get_modifier_mapping()?.reply()?;
        let mut locks = vec![ModMask::default()];
        let keys_per_modifier = modifiers.keycodes.len() / 8;
        if keys_per_modifier != 0 {
            for (index, keys) in modifiers.keycodes.chunks(keys_per_modifier).enumerate() {
                let is_lock = keys
                    .iter()
                    .filter(|&&code| code >= setup.min_keycode)
                    .any(|&code| {
                        let offset = (code - setup.min_keycode) as usize
                            * mapping.keysyms_per_keycode as usize;
                        mapping
                            .keysyms
                            .get(offset..offset + mapping.keysyms_per_keycode as usize)
                            .is_some_and(|symbols| {
                                symbols
                                    .iter()
                                    .any(|symbol| [0xffe5, 0xff7f, 0xff14].contains(symbol))
                            })
                    });
                if is_lock {
                    let mask = ModMask::from(1_u16 << index);
                    locks.extend(locks.clone().into_iter().map(|value| value | mask));
                }
            }
        }
        Ok(Self {
            connection,
            root,
            locks,
            bindings: Vec::new(),
        })
    }

    fn register(&mut self, key: HotKey) -> Result<()> {
        if self.bindings.iter().any(|binding| binding.key == key) {
            bail!("Shortcut already registered");
        }
        let name = key.key.to_string();
        let symbol = if let Some(letter) =
            name.strip_prefix("Key").filter(|letter| letter.len() == 1)
        {
            letter.as_bytes()[0] as u32
        } else if let Some(digit) = name.strip_prefix("Digit").filter(|digit| digit.len() == 1) {
            digit.as_bytes()[0] as u32
        } else if let Some(number) = name
            .strip_prefix('F')
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|number| (1..=35).contains(number))
        {
            0xffbe + number - 1
        } else {
            match name.as_str() {
                "Space" => 0x20,
                "Enter" => 0xff0d,
                "Tab" => 0xff09,
                "Escape" => 0xff1b,
                "Backspace" => 0xff08,
                "Delete" => 0xffff,
                "Insert" => 0xff63,
                "Home" => 0xff50,
                "End" => 0xff57,
                "PageUp" => 0xff55,
                "PageDown" => 0xff56,
                "ArrowLeft" => 0xff51,
                "ArrowUp" => 0xff52,
                "ArrowRight" => 0xff53,
                "ArrowDown" => 0xff54,
                _ => bail!("Unsupported X11 shortcut key: {name}"),
            }
        };
        let setup = self.connection.setup();
        let mapping = self
            .connection
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)?
            .reply()?;
        let offset = mapping
            .keysyms
            .chunks(mapping.keysyms_per_keycode as usize)
            .position(|symbols| symbols.contains(&symbol))
            .context("Shortcut key unavailable in this keyboard layout")?;
        let code = setup.min_keycode + offset as u8;
        let mut modifiers = ModMask::default();
        for (flag, mask) in [
            (Modifiers::CONTROL, ModMask::CONTROL),
            (Modifiers::ALT, ModMask::M1),
            (Modifiers::SHIFT, ModMask::SHIFT),
            (Modifiers::SUPER, ModMask::M4),
        ] {
            if key.mods.contains(flag) {
                modifiers |= mask;
            }
        }
        let result = (|| -> Result<()> {
            for lock in &self.locks {
                self.connection
                    .grab_key(
                        false,
                        self.root,
                        modifiers | *lock,
                        code,
                        GrabMode::ASYNC,
                        GrabMode::ASYNC,
                    )?
                    .check()?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            // Undo any partial lock-mask grabs before acknowledging failure.
            for lock in &self.locks {
                self.connection
                    .ungrab_key(code, self.root, modifiers | *lock)?
                    .check()?;
            }
            return Err(error);
        }
        self.bindings.push(Binding {
            key,
            code,
            modifiers,
            pressed: false,
        });
        Ok(())
    }

    fn unregister(&mut self, key: HotKey) -> Result<()> {
        if let Some(index) = self.bindings.iter().position(|binding| binding.key == key) {
            let binding = &self.bindings[index];
            // Use the keycode recorded at registration even if the layout changed.
            for lock in &self.locks {
                self.connection
                    .ungrab_key(binding.code, self.root, binding.modifiers | *lock)?
                    .check()?;
            }
            self.bindings.remove(index);
        }
        Ok(())
    }

    fn run(&mut self, commands: mpsc::Receiver<Command>) -> Result<()> {
        let lock_bits = self
            .locks
            .iter()
            .fold(0_u16, |bits, mask| bits | u16::from(*mask));
        loop {
            while let Some(event) = self.connection.poll_for_event()? {
                match event {
                    Event::KeyPress(event) => {
                        let modifiers = ModMask::from(
                            u16::from(event.state)
                                & !lock_bits
                                & u16::from(
                                    KeyButMask::SHIFT
                                        | KeyButMask::CONTROL
                                        | KeyButMask::MOD1
                                        | KeyButMask::MOD4,
                                ),
                        );
                        for binding in &mut self.bindings {
                            if binding.code == event.detail
                                && binding.modifiers == modifiers
                                && !binding.pressed
                            {
                                binding.pressed = true;
                                let _ = events().0.send(GlobalHotKeyEvent {
                                    id: binding.key.id(),
                                    state: HotKeyState::Pressed,
                                });
                            }
                        }
                    }
                    Event::KeyRelease(event) => {
                        for binding in &mut self.bindings {
                            if binding.code == event.detail && binding.pressed {
                                binding.pressed = false;
                                let _ = events().0.send(GlobalHotKeyEvent {
                                    id: binding.key.id(),
                                    state: HotKeyState::Released,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
            match commands.recv_timeout(Duration::from_millis(10)) {
                Ok(Command::Register(key, reply)) => {
                    let _ = reply.send(self.register(key));
                }
                Ok(Command::Unregister(key, reply)) => {
                    let _ = reply.send(self.unregister(key));
                }
                Ok(Command::Quit) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}
