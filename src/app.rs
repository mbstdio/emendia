use crate::{
    platform::{
        hotkey::{HotkeyRegistration, TranslationMode},
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
    session_active: bool,
}

impl Controller {
    pub fn open_smoke_preview(&mut self, cx: &mut Context<Self>) -> Result<()> {
        self.open_preview(Selection::smoke_fixture()?, cx);
        Ok(())
    }

    pub fn open_smoke_quick(&mut self, cx: &mut Context<Self>) -> Result<()> {
        self.quick_translate(Selection::smoke_fixture()?, self.settings.clone(), cx);
        // Exercise both trigger paths while the HTTP request is still pending.
        assert!(self.session_active);
        self.capture(TranslationMode::Quick, cx);
        self.capture(TranslationMode::Preview, cx);
        assert!(self.session_active);
        Ok(())
    }

    pub fn smoke_preview_complete(&self, cx: &App) -> bool {
        self.preview_window
            .as_ref()
            .and_then(|(_, view)| view.upgrade())
            .is_some_and(|view| view.read(cx).smoke_result(cx))
    }

    pub fn smoke_quick_complete(&self, cx: &App) -> bool {
        !self.session_active
            && self
                .preview_window
                .as_ref()
                .and_then(|(_, view)| view.upgrade())
                .is_some_and(|view| view.read(cx).smoke_recovery_result(cx))
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
            session_active: false,
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
            .change(&self.settings.hotkey, &self.settings.quick_hotkey)
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
                    self.hotkey
                        .change(&self.settings.hotkey, &self.settings.quick_hotkey)
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
            if event.state == HotKeyState::Pressed
                && let Some(mode) = self.hotkey.mode(event.id)
            {
                self.capture(mode, cx);
            }
        }
    }

    fn capture(&mut self, mode: TranslationMode, cx: &mut Context<Self>) {
        if self.session_active {
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
        let settings = self.settings.clone();
        let quick_key = if mode == TranslationMode::Quick {
            match settings::load_api_key(&settings.base_url) {
                Ok(key) => Some(key),
                Err(error) => {
                    self.open_settings(Some(format!("Quick Translate : {error}")), cx);
                    return;
                }
            }
        } else {
            None
        };
        self.session_active = true;
        let capture = self
            .runtime
            .spawn_blocking(move || windows::capture(target));
        cx.spawn(async move |this, cx| {
            let result = capture
                .await
                .context("Le service de capture a échoué")
                .and_then(|r| r);
            let _ = this.update(cx, |this, cx| {
                this.session_active = false;
                match result {
                    Ok(selection) => match quick_key {
                        Some(key) => this.quick_translate_with_key(selection, settings, key, cx),
                        None => this.open_preview(selection, cx),
                    },
                    Err(error) => this.open_settings(Some(format!("Capture : {error}")), cx),
                }
            });
        })
        .detach();
    }

    fn open_preview(&mut self, selection: Selection, cx: &mut Context<Self>) {
        self.open_preview_result(selection, self.settings.clone(), None, cx);
    }

    fn quick_translate(
        &mut self,
        selection: Selection,
        settings: Settings,
        cx: &mut Context<Self>,
    ) {
        let key = match settings::load_api_key(&settings.base_url) {
            Ok(key) => key,
            Err(error) => {
                self.open_settings(Some(format!("Quick Translate : {error}")), cx);
                return;
            }
        };
        self.quick_translate_with_key(selection, settings, key, cx);
    }

    fn quick_translate_with_key(
        &mut self,
        selection: Selection,
        settings: Settings,
        key: String,
        cx: &mut Context<Self>,
    ) {
        // Hold the same session gate from capture through replacement. Neither shortcut
        // may start another clipboard operation while this translation is pending.
        self.session_active = true;
        let translator = self.translator.clone();
        let runtime = self.runtime.handle().clone();
        let captured = selection.clone();
        let snapshot = settings.clone();
        let task = self.runtime.spawn(async move {
            let text = translator
                .translate(&snapshot, &key, &captured.text, None)
                .await?;
            let translated = text.clone();
            let replacement = runtime
                .spawn_blocking(move || windows::replace_quick(&captured, &translated))
                .await
                .context("Le service de remplacement a échoué")
                .and_then(|r| r);
            // Return the translation even when pasting fails, without a second API call.
            Ok::<_, anyhow::Error>((text, replacement))
        });
        cx.spawn(async move |this, cx| {
            let result = task
                .await
                .context("Le service Quick Translate a échoué")
                .and_then(|r| r);
            let _ = this.update(cx, |this, cx| {
                this.session_active = false;
                match result {
                    Ok((_, Ok(()))) => {}
                    Ok((text, Err(error))) => this.open_preview_result(
                        selection,
                        settings,
                        Some((text, format!("Quick Translate : {error}"))),
                        cx,
                    ),
                    Err(error) => {
                        this.open_settings(Some(format!("Quick Translate : {error}")), cx)
                    }
                }
            });
        })
        .detach();
    }

    fn open_preview_result(
        &mut self,
        selection: Selection,
        settings: Settings,
        result: Option<(String, String)>,
        cx: &mut Context<Self>,
    ) {
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
                        preview.update(cx, |view, cx| {
                            if let Some((text, error)) = result {
                                view.set_result(text, error, window, cx);
                            } else {
                                view.translate(false, window, cx);
                            }
                        })
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
        next.validate_hotkeys()?;
        self.hotkey
            .change_with(&next.hotkey, &next.quick_hotkey, || {
                let old_key = settings::load_api_key(&next.base_url)?;
                settings::save_api_key(&next.base_url, key)?;
                if let Err(error) = self.store.save(&next) {
                    let _ = settings::save_api_key(&next.base_url, &old_key);
                    return Err(error);
                }
                Ok(())
            })?;
        self.settings = next;
        self.tray.enabled.set_checked(true);
        Ok(())
    }
}
