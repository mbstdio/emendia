use crate::{
    platform::{
        hotkey::{HotkeyRegistration, TranslationMode},
        startup,
        tray::Tray,
        windows::{self, Selection},
    },
    settings::{self, Operation, Settings, SettingsStore, ThemePreference},
    translation::Translator,
    ui::{preview::Preview, settings::SettingsView, status::StatusView},
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
    status_window: Option<(AnyWindowHandle, WeakEntity<StatusView>)>,
    session_active: bool,
    smoke_layout_check: bool,
}

impl Controller {
    pub fn open_smoke(
        &mut self,
        operation: Operation,
        quick: bool,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.smoke_layout_check = true;
        let selection = Selection::smoke_fixture()?;
        let foreground = windows::foreground_window();
        let placement = windows::status_placement_for_window(foreground)?;
        self.open_status(placement, operation, cx)?;
        assert_eq!(
            windows::foreground_window(),
            foreground,
            "La fenêtre d’état ne doit pas prendre le focus"
        );
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            this.update(cx, |this, cx| {
                assert_eq!(
                    windows::foreground_window(),
                    foreground,
                    "L’affichage différé doit conserver le focus"
                );
                let (window, view) = this.status_window.as_ref().unwrap();
                let placement = view.upgrade().unwrap().read(cx).smoke_placement();
                assert!(
                    window
                        .update(cx, |_, window, _| StatusView::smoke_nonactivating(
                            window, placement
                        ))
                        .unwrap()
                );
                if quick {
                    let mut settings = this.settings.clone();
                    settings.correction_style = settings.quick_correction_style;
                    this.quick_translate_with_key(
                        selection,
                        settings,
                        String::new(),
                        operation,
                        cx,
                    );
                    assert!(this.session_active);
                    for mode in [
                        TranslationMode::Quick,
                        TranslationMode::Preview,
                        TranslationMode::CorrectionQuick,
                        TranslationMode::CorrectionPreview,
                    ] {
                        this.capture(mode, cx);
                    }
                    assert!(this.session_active);
                } else {
                    this.close_status(cx);
                    this.open_preview_result(selection, this.settings.clone(), operation, None, cx);
                }
            })
            .expect("Contrôleur de diagnostic");
        })
        .detach();
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
                .status_window
                .as_ref()
                .and_then(|(_, view)| view.upgrade())
                .is_some_and(|view| {
                    let view = view.read(cx);
                    view.smoke_layout();
                    view.terminal
                        && view.error
                        && view.message.starts_with("Remplacement impossible")
                })
            && self
                .preview_window
                .as_ref()
                .and_then(|(_, view)| view.upgrade())
                .is_some_and(|view| view.read(cx).smoke_recovery_result(cx))
    }

    pub fn smoke_status_closed(&self) -> bool {
        self.status_window
            .as_ref()
            .is_none_or(|(_, view)| view.upgrade().is_none())
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
            status_window: None,
            session_active: false,
            smoke_layout_check: false,
        })
    }

    pub fn start(
        &mut self,
        first_run: bool,
        initial_error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        crate::ui::theme::apply(self.settings.theme, cx);
        let error = self
            .hotkey
            .change(self.settings.shortcuts())
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
                    self.hotkey.change(self.settings.shortcuts())
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
                let status = windows::status_placement_for_window(windows::foreground_window())
                    .and_then(|placement| self.open_status(placement, mode.operation(), cx));
                if status.is_ok() {
                    self.set_status(format!("Capture : {error}"), true, true, cx);
                } else {
                    self.open_settings(Some(error.to_string()), cx);
                }
                return;
            }
        };
        let mut settings = self.settings.clone();
        if mode.quick() {
            settings.correction_style = settings.quick_correction_style;
        }
        let status = windows::status_placement(&target)
            .and_then(|placement| self.open_status(placement, mode.operation(), cx));
        if let Err(error) = status {
            self.open_settings(Some(format!("Fenêtre d’état : {error}")), cx);
            return;
        }
        let quick_key = if mode.quick() {
            match settings::load_api_key(&settings.base_url) {
                Ok(key) => Some(key),
                Err(error) => {
                    self.set_status(
                        format!("{} : {error}", mode.operation().quick_title()),
                        true,
                        true,
                        cx,
                    );
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
                        Some(key) => this.quick_translate_with_key(
                            selection,
                            settings,
                            key,
                            mode.operation(),
                            cx,
                        ),
                        None => {
                            this.close_status(cx);
                            this.open_preview_result(
                                selection,
                                settings,
                                mode.operation(),
                                None,
                                cx,
                            );
                        }
                    },
                    Err(error) => this.set_status(format!("Capture : {error}"), true, true, cx),
                }
            });
        })
        .detach();
    }

    fn close_status(&mut self, cx: &mut Context<Self>) {
        if let Some((window, _)) = self.status_window.take() {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
    }

    fn open_status(
        &mut self,
        placement: windows::Placement,
        operation: Operation,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.close_status(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(placement.x), px(placement.y)),
                size(px(placement.width), px(placement.height)),
            ))),
            display_id: Some(DisplayId::new(placement.monitor)),
            kind: WindowKind::PopUp,
            titlebar: None,
            focus: false,
            show: false,
            is_minimizable: false,
            is_resizable: false,
            ..Default::default()
        };
        let controller = cx.entity().downgrade();
        match gpui_kit::open_window(options, cx, |window, cx| {
            crate::ui::theme::configure_window(window, cx);
            let view = cx.new(|_| StatusView::new(operation, controller, placement));
            let weak = view.downgrade();
            window.on_window_should_close(cx, move |_, cx| {
                weak.upgrade().is_none_or(|view| view.read(cx).terminal)
            });
            view
        }) {
            Ok((window, view)) => {
                match window.update(cx, |_, window, cx| view.read(cx).show(window, cx)) {
                    Ok(Ok(())) => {
                        self.status_window = Some((window, view.downgrade()));
                        Ok(())
                    }
                    result => {
                        let _ = window.update(cx, |_, window, _| window.remove_window());
                        result.context("Impossible d’afficher la fenêtre d’état")?
                    }
                }
            }
            Err(error) => Err(error),
        }
    }

    fn set_status(&mut self, message: String, terminal: bool, error: bool, cx: &mut Context<Self>) {
        let Some((window, view)) = &self.status_window else {
            return;
        };
        let _ = view.update(cx, |view, cx| {
            view.message = message;
            view.terminal = terminal;
            view.error = error;
            cx.notify();
        });
        if terminal {
            let window = *window;
            cx.spawn(async move |_, cx| {
                cx.background_executor()
                    .timer(Duration::from_secs(if error { 5 } else { 2 }))
                    .await;
                let _ = window.update(cx, |_, window, _| window.remove_window());
            })
            .detach();
        }
    }

    fn quick_translate_with_key(
        &mut self,
        selection: Selection,
        settings: Settings,
        key: String,
        operation: Operation,
        cx: &mut Context<Self>,
    ) {
        // Hold the same session gate from capture through replacement. Neither shortcut
        // may start another clipboard operation while this translation is pending.
        self.session_active = true;
        self.set_status(operation.pending().into(), false, false, cx);
        let translator = self.translator.clone();
        let captured = selection.clone();
        let snapshot = settings.clone();
        let task = self.runtime.spawn(async move {
            translator
                .process(&snapshot, &key, &captured.text, None, operation)
                .await
        });
        cx.spawn(async move |this, cx| {
            let result = task
                .await
                .context("Le service de traitement rapide a échoué")
                .and_then(|r| r);
            let result = match result {
                Ok(text) => {
                    let captured = selection.clone();
                    let translated = text.clone();
                    let replacement = this.update(cx, |this, cx| {
                        this.set_status("Remplacement dans le document…".into(), false, false, cx);
                        this.runtime
                            .spawn_blocking(move || windows::replace_quick(&captured, &translated))
                    });
                    let replacement = match replacement {
                        Ok(task) => task
                            .await
                            .context("Le service de remplacement a échoué")
                            .and_then(|r| r),
                        Err(error) => Err(error),
                    };
                    Ok((text, replacement))
                }
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                this.session_active = false;
                match result {
                    Ok((_, Ok(()))) => {
                        this.set_status("Terminé — remplacement effectué.".into(), true, false, cx)
                    }
                    Ok((text, Err(error))) => {
                        this.set_status(
                            format!("Remplacement impossible : {error}"),
                            true,
                            true,
                            cx,
                        );
                        this.open_preview_result(
                            selection,
                            settings,
                            operation,
                            Some((text, format!("{} : {error}", operation.quick_title()))),
                            cx,
                        )
                    }
                    Err(error) => this.set_status(
                        format!("{} : {error}", operation.quick_title()),
                        true,
                        true,
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    fn open_preview_result(
        &mut self,
        selection: Selection,
        settings: Settings,
        operation: Operation,
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
                title: Some(operation.title().into()),
                ..Default::default()
            }),
            is_minimizable: false,
            window_min_size: Some(size(px(360.), px(320.))),
            ..Default::default()
        };
        let controller = cx.entity().downgrade();
        let translator = self.translator.clone();
        let runtime = self.runtime.handle().clone();
        let smoke_layout_check = self.smoke_layout_check;
        match gpui_kit::open_window(options, cx, |window, cx| {
            crate::ui::theme::configure_window(window, cx);
            cx.new(|cx| {
                Preview::new(
                    selection,
                    (settings, operation),
                    translator,
                    runtime,
                    controller,
                    window,
                    cx,
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
                            if smoke_layout_check {
                                view.enable_smoke_layout();
                            }
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
            window_bounds: Some(WindowBounds::centered(size(px(860.), px(680.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("Translation Tool — Paramètres".into()),
                ..Default::default()
            }),
            window_min_size: Some(size(px(720.), px(500.))),
            ..Default::default()
        };
        match gpui_kit::open_window(options, cx, |window, cx| {
            crate::ui::theme::configure_window(window, cx);
            cx.new(|cx| SettingsView::new(settings, controller, error, window, cx))
        }) {
            Ok((window, view)) => self.settings_window = Some((window, view.downgrade())),
            Err(error) => tracing::error!(%error, "Impossible d’ouvrir les paramètres"),
        }
    }

    pub fn save_settings(&mut self, next: Settings, key: &str) -> Result<()> {
        next.validate()?;
        next.validate_hotkeys()?;
        self.hotkey.change_with(next.shortcuts(), || {
            startup::configure(next.launch_at_startup, || {
                let old_key = settings::load_api_key(&next.base_url)?;
                settings::save_api_key(&next.base_url, key)?;
                if let Err(error) = self.store.save(&next) {
                    let _ = settings::save_api_key(&next.base_url, &old_key);
                    return Err(error);
                }
                Ok(())
            })
        })?;
        self.settings = next;
        self.tray.enabled.set_checked(true);
        Ok(())
    }

    pub fn set_theme(&mut self, theme: ThemePreference, cx: &mut Context<Self>) -> Result<()> {
        self.settings = self.store.save_theme(&self.settings, theme)?;
        crate::ui::theme::apply(theme, cx);
        Ok(())
    }
}
