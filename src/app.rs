use crate::{
    platform::{
        hotkey::HotkeyRegistration,
        tray::Tray,
        windows::{self, Selection},
    },
    settings::{self, Settings, SettingsStore},
    translation::Translator,
    ui::{preview::Preview, settings::SettingsView},
};
use anyhow::{Context as _, Result};
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use gpui_kit::*;
use std::time::Duration;
use tokio::runtime::Runtime;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent, menu::MenuEvent};

pub struct Controller {
    pub settings: Settings,
    pub translator: Translator,
    pub runtime: Runtime,
    store: SettingsStore,
    hotkey: HotkeyRegistration,
    tray: Tray,
    settings_window: Option<(AnyWindowHandle, WeakEntity<SettingsView>)>,
    preview_window: Option<(AnyWindowHandle, WeakEntity<Preview>)>,
    capturing: bool,
}

impl Controller {
    pub fn open_smoke_preview(&mut self, cx: &mut Context<Self>) -> Result<()> {
        self.open_preview(Selection::smoke_fixture()?, cx);
        Ok(())
    }

    pub fn smoke_preview_complete(&self, cx: &App) -> bool {
        self.preview_window
            .as_ref()
            .and_then(|(_, view)| view.upgrade())
            .is_some_and(|view| view.read(cx).smoke_result(cx))
    }

    pub fn new(
        settings: Settings,
        store: SettingsStore,
        runtime: Runtime,
        translator: Translator,
    ) -> Result<Self> {
        Ok(Self {
            settings,
            store,
            runtime,
            translator,
            hotkey: HotkeyRegistration::new()?,
            tray: Tray::new()?,
            settings_window: None,
            preview_window: None,
            capturing: false,
        })
    }

    pub fn start(
        &mut self,
        first_run: bool,
        initial_error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let error = self
            .hotkey
            .change(&self.settings.hotkey)
            .err()
            .map(|e| e.to_string())
            .or_else(|| self.settings.validate().err().map(|e| e.to_string()))
            .or(initial_error);
        self.tray.enabled.set_checked(self.hotkey.enabled());
        if first_run || error.is_some() {
            self.open_settings(error, cx);
        }
        // Tray and hotkey libraries post Win32 messages on this same thread. Drain their
        // non-blocking receivers from GPUI; the timer also works with zero open windows.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(30))
                    .await;
                if this.update(cx, |this, cx| this.poll_events(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_events(&mut self, cx: &mut Context<Self>) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == *self.tray.quit.id() {
                cx.quit();
            } else if event.id == *self.tray.settings.id() {
                self.open_settings(None, cx);
            } else if event.id == *self.tray.enabled.id() {
                let result = if self.hotkey.enabled() {
                    self.hotkey.disable()
                } else {
                    self.hotkey.change(&self.settings.hotkey)
                };
                self.tray.enabled.set_checked(self.hotkey.enabled());
                if let Err(error) = result {
                    self.open_settings(Some(error.to_string()), cx);
                }
            }
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                self.open_settings(None, cx);
            }
        }
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == HotKeyState::Pressed && self.hotkey.matches(event.id) {
                self.capture(cx);
            }
        }
    }

    fn capture(&mut self, cx: &mut Context<Self>) {
        if self.capturing {
            return;
        }
        if let Some((handle, _)) = &self.preview_window {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
            {
                return;
            }
            self.preview_window = None;
        }
        let target = match windows::capture_target() {
            Ok(target) => target,
            Err(error) => {
                self.open_settings(Some(error.to_string()), cx);
                return;
            }
        };
        self.capturing = true;
        let capture = self
            .runtime
            .spawn_blocking(move || windows::capture(target));
        cx.spawn(async move |this, cx| {
            let result = capture
                .await
                .context("Le service de capture a échoué")
                .and_then(|r| r);
            let _ = this.update(cx, |this, cx| {
                this.capturing = false;
                match result {
                    Ok(selection) => this.open_preview(selection, cx),
                    Err(error) => this.open_settings(Some(format!("Capture : {error}")), cx),
                }
            });
        })
        .detach();
    }

    fn open_preview(&mut self, selection: Selection, cx: &mut Context<Self>) {
        let placement = selection.placement;
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(placement.x), px(placement.y)),
                size(px(placement.width), px(placement.height)),
            ))),
            display_id: Some(DisplayId::new(placement.monitor)),
            kind: WindowKind::PopUp,
            titlebar: Some(TitlebarOptions {
                title: Some("Traduction".into()),
                ..Default::default()
            }),
            is_minimizable: false,
            window_min_size: Some(size(px(360.), px(320.))),
            ..Default::default()
        };
        let controller = cx.entity().downgrade();
        let settings = self.settings.clone();
        let translator = self.translator.clone();
        let runtime = self.runtime.handle().clone();
        match gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| {
                Preview::new(
                    selection, settings, translator, runtime, controller, window, cx,
                )
            })
        }) {
            Ok((window, preview)) => {
                self.preview_window = Some((window, preview.downgrade()));
                // The preview reads Controller to pick up current provider settings.
                // Defer until this Controller update has released its GPUI entity lease.
                cx.defer(move |cx| {
                    let _ = window.update(cx, |_, window, cx| {
                        preview.update(cx, |view, cx| view.translate(false, window, cx))
                    });
                });
            }
            Err(error) => self.open_settings(Some(format!("Ouverture de l’aperçu : {error}")), cx),
        }
    }

    pub fn open_settings(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        if let Some((handle, view)) = &self.settings_window
            && handle
                .update(cx, |_, window, cx| {
                    window.activate_window();
                    if let Some(error) = error.clone() {
                        let _ = view.update(cx, |view, cx| {
                            view.status = error;
                            cx.notify();
                        });
                    }
                })
                .is_ok()
        {
            return;
        }
        let settings = self.settings.clone();
        let controller = cx.entity().downgrade();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(650.), px(680.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("Translation Tool — Paramètres".into()),
                ..Default::default()
            }),
            window_min_size: Some(size(px(480.), px(500.))),
            ..Default::default()
        };
        match gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| SettingsView::new(settings, controller, error, window, cx))
        }) {
            Ok((window, view)) => self.settings_window = Some((window, view.downgrade())),
            Err(error) => tracing::error!(%error, "Impossible d’ouvrir les paramètres"),
        }
    }

    pub fn save_settings(&mut self, next: Settings, key: &str) -> Result<()> {
        next.validate()?;
        let previous = self.settings.clone();
        let was_enabled = self.hotkey.enabled();
        self.hotkey.change(&next.hotkey)?;
        let saved = (|| {
            let old_key = settings::load_api_key(&next.base_url)?;
            settings::save_api_key(&next.base_url, key)?;
            if let Err(error) = self.store.save(&next) {
                let _ = settings::save_api_key(&next.base_url, &old_key);
                return Err(error);
            }
            Ok(())
        })();
        if let Err(error) = saved {
            if was_enabled {
                let _ = self.hotkey.change(&previous.hotkey);
            } else {
                let _ = self.hotkey.disable();
            }
            return Err(error);
        }
        self.settings = next;
        self.tray.enabled.set_checked(true);
        Ok(())
    }
}
