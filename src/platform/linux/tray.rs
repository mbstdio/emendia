use crate::i18n::t;
use anyhow::{Context, Result};
use std::{sync::mpsc, thread, time::Duration};
use tray_icon::{
    Icon, TrayIconBuilder,
    menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem},
};

enum Command {
    Enabled(bool),
    Localize,
    Icon(Icon),
    Quit,
}

pub struct Tray {
    pub settings: MenuId,
    pub enabled: MenuId,
    pub quit: MenuId,
    commands: Option<mpsc::Sender<Command>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Tray {
    pub fn new(initial_icon: Icon) -> Result<Self> {
        // A successfully constructed indicator does not imply a desktop can show it.
        // Keep a normal window available on GNOME without the indicator extension.
        if !has_indicator_host() {
            tracing::warn!("No StatusNotifierWatcher; closing the last window will quit Emendia");
            return Ok(Self::unavailable());
        }
        let (commands, receiver) = mpsc::channel();
        let (ready, result) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("emendia-tray".into())
            .spawn(move || {
                let initialized = (|| -> Result<_> {
                    gtk::init().context("Unable to initialize the GTK tray loop")?;
                    let menu = Menu::new();
                    let settings = MenuItem::new(t("Settings"), true, None);
                    let enabled = CheckMenuItem::new(t("Shortcuts enabled"), true, true, None);
                    let quit = MenuItem::new(t("Quit"), true, None);
                    menu.append_items(&[
                        &settings,
                        &enabled,
                        &PredefinedMenuItem::separator(),
                        &quit,
                    ])?;
                    let icon = TrayIconBuilder::new()
                        .with_tooltip(t("Emendia — translation and proofreading"))
                        .with_icon(initial_icon)
                        .with_menu(Box::new(menu))
                        .build()?;
                    Ok((icon, settings, enabled, quit))
                })();
                let (icon, settings, enabled, quit) = match initialized {
                    Ok(values) => values,
                    Err(error) => {
                        let _ = ready.send(Err(error.to_string()));
                        return;
                    }
                };
                if ready
                    .send(Ok((
                        settings.id().clone(),
                        enabled.id().clone(),
                        quit.id().clone(),
                    )))
                    .is_err()
                {
                    return;
                }
                // GTK objects stay exclusively on this thread. GPUI only receives IDs
                // and sends commands; no GTK main-loop work runs on its renderer thread.
                gtk::glib::timeout_add_local(Duration::from_millis(30), move || {
                    loop {
                        match receiver.try_recv() {
                            Ok(Command::Enabled(value)) => enabled.set_checked(value),
                            Ok(Command::Icon(next)) => {
                                if let Err(error) = icon.set_icon(Some(next)) {
                                    tracing::warn!(%error, "Unable to update the tray icon");
                                }
                            }
                            Ok(Command::Localize) => {
                                settings.set_text(t("Settings"));
                                enabled.set_text(t("Shortcuts enabled"));
                                quit.set_text(t("Quit"));
                                let _ = icon
                                    .set_tooltip(Some(t("Emendia — translation and proofreading")));
                            }
                            Ok(Command::Quit) | Err(mpsc::TryRecvError::Disconnected) => {
                                gtk::main_quit();
                                return gtk::glib::ControlFlow::Break;
                            }
                            Err(mpsc::TryRecvError::Empty) => break,
                        }
                    }
                    gtk::glib::ControlFlow::Continue
                });
                gtk::main();
            })?;
        match result.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok((settings, enabled, quit))) => Ok(Self {
                settings,
                enabled,
                quit,
                commands: Some(commands),
                thread: Some(thread),
            }),
            error => {
                tracing::warn!(?error, "Tray unavailable; using normal window lifetime");
                drop(commands);
                Ok(Self::unavailable())
            }
        }
    }

    fn unavailable() -> Self {
        Self {
            settings: MenuId::new("settings"),
            enabled: MenuId::new("enabled"),
            quit: MenuId::new("quit"),
            commands: None,
            thread: None,
        }
    }

    pub fn available(&self) -> bool {
        self.commands.is_some()
    }
    pub fn set_enabled(&self, value: bool) {
        if let Some(sender) = &self.commands {
            let _ = sender.send(Command::Enabled(value));
        }
    }
    pub fn localize(&self) {
        if let Some(sender) = &self.commands {
            let _ = sender.send(Command::Localize);
        }
    }

    pub fn set_icon(&self, icon: Icon) -> Result<()> {
        if let Some(sender) = &self.commands {
            sender
                .send(Command::Icon(icon))
                .context("GTK tray service unavailable")?;
        }
        Ok(())
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        if let Some(sender) = &self.commands {
            let _ = sender.send(Command::Quit);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn has_indicator_host() -> bool {
    let Ok(connection) = dbus::blocking::Connection::new_session() else {
        return false;
    };
    let proxy = connection.with_proxy(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        Duration::from_millis(500),
    );
    let result: std::result::Result<(bool,), _> = proxy.method_call(
        "org.freedesktop.DBus",
        "NameHasOwner",
        ("org.kde.StatusNotifierWatcher",),
    );
    result.is_ok_and(|(present,)| present)
}
