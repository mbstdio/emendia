use crate::i18n::{UiLanguage, t};
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
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
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
            "The status window must not take focus"
        );
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            this.update(cx, |this, cx| {
                assert_eq!(
                    windows::foreground_window(),
                    foreground,
                    "Deferred display must preserve focus"
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
            .expect("Smoke test controller");
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
                        && view.message.starts_with(t("Replacement unavailable"))
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
        gpui_kit::component::set_locale(if t("Settings") == "Settings" {
            "en"
        } else {
            "fr"
        });
        let error = self
            .settings
            .onboarding_completed
            .then(|| self.hotkey.change(self.settings.shortcuts()))
            .and_then(Result::err)
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
                if !self.settings.onboarding_completed {
                    self.open_settings(None, cx);
                    continue;
                }
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
        if !self.settings.onboarding_completed {
            self.open_settings(None, cx);
            return;
        }
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
                    self.set_status(format!("{}: {error}", t("Capture")), true, true, cx);
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
            self.open_settings(Some(format!("{}: {error}", t("Status window"))), cx);
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
                .context(t("The capture service failed"))
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
                    Err(error) => {
                        this.set_status(format!("{}: {error}", t("Capture")), true, true, cx)
                    }
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
                        result.context(t("Unable to show the status window"))?
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
                .context(t("The quick processing service failed"))
                .and_then(|r| r);
            let result = match result {
                Ok(text) => {
                    let captured = selection.clone();
                    let translated = text.clone();
                    let replacement = this.update(cx, |this, cx| {
                        this.set_status(t("Replacing in the document…").into(), false, false, cx);
                        this.runtime
                            .spawn_blocking(move || windows::replace_quick(&captured, &translated))
                    });
                    let replacement = match replacement {
                        Ok(task) => task
                            .await
                            .context(t("The replacement service failed"))
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
                        this.set_status(t("Done — text replaced.").into(), true, false, cx)
                    }
                    Ok((text, Err(error))) => {
                        this.set_status(
                            format!("{}: {error}", t("Replacement unavailable")),
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
            Err(error) => {
                self.open_settings(Some(format!("{}: {error}", t("Opening preview"))), cx)
            }
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
        let onboarding = !settings.onboarding_completed;
        let provider_configured = self
            .store
            .load()
            .is_ok_and(|saved| saved.is_some_and(|settings| settings.validate().is_ok()));
        let controller = cx.entity().downgrade();
        // Use the launch/tray-click monitor for both placement and DPI conversion.
        let display_id = windows::cursor_monitor()
            .ok()
            .map(DisplayId::new)
            .filter(|id| cx.find_display(*id).is_some());
        let window_bounds = Bounds::centered(display_id, size(px(860.), px(680.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(window_bounds)),
            display_id,
            is_resizable: !onboarding,
            titlebar: Some(TitlebarOptions {
                title: Some(
                    t(if onboarding {
                        "Emendia — Welcome"
                    } else {
                        "Emendia — Settings"
                    })
                    .into(),
                ),
                ..Default::default()
            }),
            window_min_size: Some(size(px(720.), px(500.))),
            ..Default::default()
        };
        match gpui_kit::open_window(options, cx, |window, cx| {
            // Windows can rescale the initial placement when creation crosses monitors.
            // Apply the logical client size once the destination DPI is established.
            if let Ok(handle) = HasWindowHandle::window_handle(window)
                && let RawWindowHandle::Win32(handle) = handle.as_raw()
                && let Err(error) = windows::center_window(
                    handle.hwnd.get(),
                    window_bounds.size.width.as_f32(),
                    window_bounds.size.height.as_f32(),
                )
            {
                tracing::debug!(%error, "Unable to apply the final window bounds");
            }
            crate::ui::theme::configure_window(window, cx);
            cx.new(|cx| {
                SettingsView::new(settings, controller, error, provider_configured, window, cx)
            })
        }) {
            Ok((window, view)) => self.settings_window = Some((window, view.downgrade())),
            Err(error) => tracing::error!(%error, "Unable to open settings"),
        }
    }

    pub fn smoke_onboarding_step(&mut self, step: usize, cx: &mut Context<Self>) {
        assert!(!self.settings.onboarding_completed);
        let expected_display = windows::cursor_monitor().ok().map(DisplayId::new);
        self.open_settings(None, cx);
        let (handle, view) = self
            .settings_window
            .as_ref()
            .expect("Setup window must exist");
        handle
            .update(cx, |_, window, cx| {
                if step == 0
                    && let Some(expected_display) = expected_display
                {
                    assert_eq!(
                        window.display(cx).map(|display| display.id()),
                        Some(expected_display),
                        "Setup must open on the cursor monitor, using that monitor's DPI"
                    );
                }
                view.update(cx, |view, cx| view.smoke_onboarding_step(step, cx))
                    .expect("Setup view must exist");
            })
            .expect("Setup window must remain open");
        assert_eq!(cx.windows().len(), 1, "Only onboarding must be open");
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

    pub fn set_language(&mut self, language: UiLanguage, cx: &mut Context<Self>) -> Result<()> {
        self.settings = self.store.save_language(&self.settings, language)?;
        crate::i18n::apply(language);
        gpui_kit::component::set_locale(if t("Settings") == "Settings" {
            "en"
        } else {
            "fr"
        });
        self.tray.localize();
        let preview = self.preview_window.clone();
        let status = self.status_window.clone();
        // The caller holds the SettingsView lease until this update returns.
        cx.defer(move |cx| {
            if let Some((handle, view)) = preview {
                let _ = handle.update(cx, |_, window, cx| {
                    let _ = view.update(cx, |view, cx| view.localize(window, cx));
                });
            }
            if let Some((_, view)) = status {
                let _ = view.update(cx, |_, cx| cx.notify());
            }
        });
        cx.refresh_windows();
        Ok(())
    }
}
