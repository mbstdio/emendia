use crate::i18n::{UiLanguage, canonical_language, t};
use crate::{
    app::Controller,
    platform::hotkey::{self, TranslationMode},
    settings::{self, CorrectionStyle, Settings, ThemePreference},
    ui::{LanguageSelect, language_select, style_buttons, text_input},
};
use anyhow::{Context as _, Result};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        input::{Input, InputEvent, InputState},
        select::{Select, SelectEvent, SelectState},
        *,
    },
    *,
};
use tokio::task::AbortHandle;

const PROVIDERS: &[&str] = &["OpenAI", "LM Studio", "Ollama", "Custom"];
const ONBOARDING_HERO_HEIGHT: f32 = 310.;
const ONBOARDING_TITLE_BAR_HEIGHT: f32 = if cfg!(target_os = "windows") { 34. } else { 0. };
const LOGO_LIGHT: &[u8] = include_bytes!("../ressources/logo.png");
const LOGO_DARK: &[u8] = include_bytes!("../ressources/logo_w.png");

fn resized_logo(bytes: &[u8], size: u32) -> std::sync::Arc<RenderImage> {
    let mut buffer = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .expect("Embedded logo must be a valid PNG")
        .resize(size, size, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    // GPUI's RenderImage expects BGRA pixels.
    for pixel in buffer.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    std::sync::Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

struct ShortcutReassignment {
    shortcut: String,
    target: TranslationMode,
    displaced: Vec<TranslationMode>,
}

struct ShortcutDraft {
    values: [String; 4],
    reassignment: Option<ShortcutReassignment>,
}

impl ShortcutDraft {
    fn new(settings: &Settings) -> Self {
        Self {
            values: settings.shortcuts().map(str::to_owned),
            reassignment: None,
        }
    }

    fn assign(&mut self, target: TranslationMode, candidate: String) -> Result<()> {
        let key = hotkey::parse(&candidate)?;
        let mut displaced = Vec::new();
        for mode in TranslationMode::ALL {
            if mode != target && hotkey::parse(&self.values[mode.index()]).ok() == Some(key) {
                self.values[mode.index()].clear();
                displaced.push(mode);
            }
        }
        self.values[target.index()] = candidate.clone();
        self.reassignment = (!displaced.is_empty()).then_some(ShortcutReassignment {
            shortcut: candidate,
            target,
            displaced,
        });
        Ok(())
    }

    fn complete(&self) -> bool {
        self.values.iter().all(|value| !value.trim().is_empty())
    }

    fn warning(&self) -> Option<String> {
        if let Some(reassignment) = &self.reassignment {
            let actions = reassignment
                .displaced
                .iter()
                .map(|mode| mode.label())
                .collect::<Vec<_>>()
                .join(", ");
            Some(t("Shortcut {shortcut} was already assigned to {actions}. It is now assigned to {target}. Reassign the actions without a shortcut before saving.")
                .replace("{shortcut}", &reassignment.shortcut)
                .replace("{actions}", &actions)
                .replace("{target}", reassignment.target.label()))
        } else if !self.complete() {
            Some(t("Assign a shortcut to every action before saving.").to_owned())
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    General,
    Provider,
    Translation,
    Correction,
    Shortcuts,
}

impl Category {
    const ALL: [Self; 5] = [
        Self::General,
        Self::Provider,
        Self::Translation,
        Self::Correction,
        Self::Shortcuts,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::General => t("General"),
            Self::Provider => t("AI Provider"),
            Self::Translation => t("Translation"),
            Self::Correction => t("Proofreading"),
            Self::Shortcuts => t("Shortcuts"),
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::General => t("Customize the application's appearance and startup."),
            Self::Provider => t("Configure the model used to translate and proofread your text."),
            Self::Translation => t("Choose translation languages and shortcuts."),
            Self::Correction => t("Adjust proofreading styles and shortcuts."),
            Self::Shortcuts => t("Find all your global shortcuts in one place."),
        }
    }

    fn icon(self) -> gpui_kit::assets::IconName {
        use gpui_kit::assets::IconName;
        match self {
            Self::General => IconName::Settings,
            Self::Provider => IconName::Server,
            Self::Translation => IconName::Languages,
            Self::Correction => IconName::SpellCheck,
            Self::Shortcuts => IconName::Keyboard,
        }
    }
}

pub struct SettingsView {
    controller: WeakEntity<Controller>,
    provider: Entity<SelectState<Vec<String>>>,
    base_url: Entity<InputState>,
    model: Entity<InputState>,
    api_key: Entity<InputState>,
    source: LanguageSelect,
    target: LanguageSelect,
    shortcuts: ShortcutDraft,
    style: CorrectionStyle,
    quick_style: CorrectionStyle,
    launch_at_startup: bool,
    theme: LanguageSelect,
    theme_preference: ThemePreference,
    language: LanguageSelect,
    ui_language: UiLanguage,
    category: Category,
    recording: Option<TranslationMode>,
    focus: FocusHandle,
    pub status: String,
    testing: bool,
    pending: Option<AbortHandle>,
    test_version: u64,
    _subscriptions: Vec<Subscription>,
    onboarding_step: Option<usize>,
    hero: std::sync::Arc<Image>,
    logo_light: std::sync::Arc<RenderImage>,
    logo_dark: std::sync::Arc<RenderImage>,
    logo_size: u32,
}

impl SettingsView {
    pub fn new(
        settings: Settings,
        controller: WeakEntity<Controller>,
        error: Option<String>,
        provider_configured: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.on_release(|this, cx| {
            let controller = this.controller.clone();
            cx.defer(move |cx| {
                if controller
                    .upgrade()
                    .is_some_and(|app| !app.read(cx).settings.onboarding_completed)
                {
                    cx.quit();
                }
            });
        })
        .detach();
        let preset = match settings.base_url.trim_end_matches('/') {
            "https://api.openai.com/v1" => 0,
            "http://localhost:1234/v1" => 1,
            "http://localhost:11434/v1" => 2,
            _ => 3,
        };
        let provider = cx.new(|cx| {
            SelectState::new(
                PROVIDERS
                    .iter()
                    .map(|p| t(p).into())
                    .collect::<Vec<String>>(),
                Some(IndexPath::new(preset)),
                window,
                cx,
            )
        });
        let base_url = text_input(&settings.base_url, window, cx);
        let model = text_input(&settings.model, window, cx);
        let (key, key_error) = match settings::load_api_key(&settings.base_url) {
            Ok(key) => (key, None),
            Err(e) => (String::new(), Some(e.to_string())),
        };
        let api_key = cx.new(|cx| {
            let mut input = InputState::new(window, cx)
                .masked(true)
                .placeholder(t("Optional for a local server"));
            input.set_value(key, window, cx);
            input
        });
        let source = language_select(&settings.source_language, true, window, cx);
        let target = language_select(&settings.target_language, false, window, cx);
        let style = settings.correction_style;
        let quick_style = settings.quick_correction_style;
        let theme = cx.new(|cx| {
            SelectState::new(
                ThemePreference::ALL
                    .into_iter()
                    .map(|theme| theme.label().to_owned())
                    .collect::<Vec<_>>(),
                ThemePreference::ALL
                    .iter()
                    .position(|theme| *theme == settings.theme)
                    .map(IndexPath::new),
                window,
                cx,
            )
        });
        let language = cx.new(|cx| {
            SelectState::new(
                UiLanguage::ALL
                    .into_iter()
                    .map(|v| v.label().to_owned())
                    .collect::<Vec<_>>(),
                UiLanguage::ALL
                    .iter()
                    .position(|v| *v == settings.ui_language)
                    .map(IndexPath::new),
                window,
                cx,
            )
        });
        let mut subscriptions = vec![
            cx.subscribe_in(
                &language,
                window,
                |this, select, event: &SelectEvent<Vec<String>>, window, cx| {
                    if let SelectEvent::Confirm(Some(_)) = event {
                        let Some(index) = select.read(cx).selected_index(cx) else {
                            return;
                        };
                        let Some(language) = UiLanguage::ALL.get(index.row).copied() else {
                            return;
                        };
                        match this
                            .controller
                            .update(cx, |app, cx| app.set_language(language, cx))
                            .and_then(|r| r)
                        {
                            Ok(()) => {
                                this.ui_language = language;
                                this.localize(window, cx);
                                this.status = t("Interface language applied and saved.").into();
                            }
                            Err(error) => {
                                this.language.update(cx, |state, cx| {
                                    state.set_selected_index(
                                        UiLanguage::ALL
                                            .iter()
                                            .position(|v| *v == this.ui_language)
                                            .map(IndexPath::new),
                                        window,
                                        cx,
                                    )
                                });
                                this.status = error.to_string();
                            }
                        }
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &theme,
                window,
                |this, _, event: &SelectEvent<Vec<String>>, window, cx| {
                    if let SelectEvent::Confirm(Some(label)) = event {
                        let Some(theme) = ThemePreference::from_label(label) else {
                            return;
                        };
                        if theme == this.theme_preference {
                            return;
                        }
                        match this
                            .controller
                            .update(cx, |app, cx| app.set_theme(theme, cx))
                            .and_then(|result| result)
                        {
                            Ok(()) => {
                                this.theme_preference = theme;
                                this.status = t("Theme applied and saved.").into();
                            }
                            Err(error) => {
                                let index = ThemePreference::ALL
                                    .iter()
                                    .position(|theme| *theme == this.theme_preference)
                                    .map(IndexPath::new);
                                this.theme.update(cx, |state, cx| {
                                    state.set_selected_index(index, window, cx)
                                });
                                this.status = error.to_string();
                            }
                        }
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &provider,
                window,
                |this, _, event: &SelectEvent<Vec<String>>, window, cx| {
                    if let SelectEvent::Confirm(Some(provider)) = event {
                        let preset = match provider.as_str() {
                            "OpenAI" => Some(("https://api.openai.com/v1", "gpt-6-luna")),
                            "LM Studio" => Some(("http://localhost:1234/v1", "local-model")),
                            "Ollama" => Some(("http://localhost:11434/v1", "llama3.2")),
                            _ => None,
                        };
                        this.cancel_test();
                        if let Some((url, model)) = preset {
                            this.base_url
                                .update(cx, |state, cx| state.set_value(url, window, cx));
                            this.model
                                .update(cx, |state, cx| state.set_value(model, window, cx));
                            this.reload_key(window, cx);
                            this.status =
                                t("Enter the exact model name available from this provider.")
                                    .into();
                        }
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &base_url,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.cancel_test();
                        this.reload_key(window, cx);
                        cx.notify();
                    }
                },
            ),
        ];
        for input in [&model, &api_key] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.cancel_test();
                        cx.notify();
                    }
                },
            ));
        }
        for select in [&source, &target] {
            subscriptions.push(cx.subscribe_in(
                select,
                window,
                |this, _, _: &SelectEvent<Vec<String>>, _, cx| {
                    this.cancel_test();
                    cx.notify();
                },
            ));
        }
        let logo_size = ((if settings.onboarding_completed { 36. } else { 40. })
            * window.scale_factor())
        .round()
        .max(1.) as u32;
        Self {
            onboarding_step: (!settings.onboarding_completed).then_some(0),
            hero: std::sync::Arc::new(Image::from_bytes(
                ImageFormat::Jpeg,
                include_bytes!("../ressources/onboard-hero.jpg").to_vec(),
            )),
            logo_light: resized_logo(LOGO_LIGHT, logo_size),
            logo_dark: resized_logo(LOGO_DARK, logo_size),
            logo_size,
            controller,
            provider,
            base_url,
            model,
            api_key,
            source,
            target,
            shortcuts: ShortcutDraft::new(&settings),
            style,
            quick_style,
            launch_at_startup: settings.launch_at_startup,
            theme,
            theme_preference: settings.theme,
            language,
            ui_language: settings.ui_language,
            category: Category::General,
            recording: None,
            focus: cx.focus_handle(),
            status: error.or(key_error).unwrap_or_else(|| {
                if provider_configured || !settings.onboarding_completed {
                    String::new()
                } else {
                    t("Configure the provider, then test the connection and save.").into()
                }
            }),
            testing: false,
            pending: None,
            test_version: 0,
            _subscriptions: subscriptions,
        }
    }

    fn reload_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = self.base_url.read(cx).value().to_string();
        match settings::load_api_key(settings::normalize_endpoint(&url)) {
            Ok(key) => self
                .api_key
                .update(cx, |state, cx| state.set_value(key, window, cx)),
            Err(error) => {
                self.api_key
                    .update(cx, |state, cx| state.set_value("", window, cx));
                self.status = error.to_string();
            }
        }
    }

    fn cancel_test(&mut self) {
        self.test_version += 1;
        if let Some(task) = self.pending.take() {
            task.abort();
        }
        if self.testing {
            self.status = t("Configuration changed; run the connection test again.").into();
        }
        self.testing = false;
    }

    fn values(&self, cx: &App) -> Result<(Settings, String)> {
        let settings = Settings {
            onboarding_completed: true,
            base_url: self
                .base_url
                .read(cx)
                .value()
                .trim()
                .trim_end_matches('/')
                .into(),
            model: self.model.read(cx).value().trim().into(),
            source_language: self
                .source
                .read(cx)
                .selected_value()
                .map(|value| canonical_language(value).to_owned())
                .context(t("Choose the source language"))?,
            target_language: self
                .target
                .read(cx)
                .selected_value()
                .map(|value| canonical_language(value).to_owned())
                .context(t("Choose the target language"))?,
            hotkey: self.shortcuts.values[0].clone(),
            quick_hotkey: self.shortcuts.values[1].clone(),
            correction_hotkey: self.shortcuts.values[2].clone(),
            quick_correction_hotkey: self.shortcuts.values[3].clone(),
            correction_style: self.style,
            quick_correction_style: self.quick_style,
            launch_at_startup: self.launch_at_startup,
            theme: self.theme_preference,
            ui_language: self.ui_language,
        };
        settings.validate()?;
        settings.validate_hotkeys()?;
        let key = self.api_key.read(cx).value().trim().to_owned();
        if self.onboarding_step.is_some()
            && settings.base_url == "https://api.openai.com/v1"
            && key.is_empty()
        {
            anyhow::bail!(t("Enter your OpenAI API key, or choose another provider."));
        }
        Ok((settings, key))
    }

    fn save(&mut self, cx: &mut Context<Self>) -> bool {
        let result = self.values(cx).and_then(|(settings, key)| {
            self.controller
                .update(cx, |app, _| app.save_settings(settings, &key))?
        });
        let success = result.is_ok();
        self.status = match result {
            Ok(()) => t("Settings saved. Shortcuts are active; you can close this window.").into(),
            Err(error) => error.to_string(),
        };
        cx.notify();
        success
    }

    fn test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (settings, key) = match self.values(cx) {
            Ok(values) => values,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        self.cancel_test();
        let version = self.test_version;
        let result = self.controller.update(cx, |app, _| {
            let translator = app.translator.clone();
            app.runtime.spawn(async move {
                translator
                    .translate(
                        &settings,
                        &key,
                        "Bonjour, ceci est un test de traduction.",
                        None,
                    )
                    .await
            })
        });
        let task = match result {
            Ok(task) => task,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        self.pending = Some(task.abort_handle());
        self.testing = true;
        self.status = t("Testing connection…").into();
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                if this.test_version != version {
                    return;
                }
                this.pending = None;
                this.testing = false;
                this.status = match result {
                    Ok(Ok(text)) => format!("{}: {text}", t("Connection successful")),
                    Ok(Err(error)) => error.to_string(),
                    Err(_) => t("Test interrupted.").into(),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn record(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(mode) = self.recording else {
            return;
        };
        cx.stop_propagation();
        let stroke = &event.keystroke;
        if stroke.key == "escape" {
            self.recording = None;
            cx.notify();
            return;
        }
        let key = if stroke.key.len() == 1 && stroke.key.chars().all(|c| c.is_ascii_alphabetic()) {
            format!("Key{}", stroke.key.to_ascii_uppercase())
        } else if stroke.key.len() == 1 && stroke.key.chars().all(|c| c.is_ascii_digit()) {
            format!("Digit{}", stroke.key)
        } else if stroke.key.starts_with('f')
            && stroke.key[1..]
                .parse::<u8>()
                .is_ok_and(|n| (1..=24).contains(&n))
        {
            stroke.key.to_ascii_uppercase()
        } else {
            self.status = t("Use a letter, digit or F1–F24 with Ctrl, Alt or Super/Win.").into();
            cx.notify();
            return;
        };
        let mut parts = Vec::new();
        if stroke.modifiers.control {
            parts.push("Ctrl".to_owned());
        }
        if stroke.modifiers.alt {
            parts.push("Alt".to_owned());
        }
        if stroke.modifiers.platform {
            parts.push("Super".to_owned());
        }
        if stroke.modifiers.shift {
            parts.push("Shift".to_owned());
        }
        parts.push(key);
        let candidate = parts.join("+");
        self.capture_shortcut(mode, candidate, cx);
    }

    fn capture_shortcut(
        &mut self,
        mode: TranslationMode,
        candidate: String,
        cx: &mut Context<Self>,
    ) {
        match self.shortcuts.assign(mode, candidate) {
            Ok(()) => {
                self.recording = None;
                self.status = t("Shortcut captured. Save to activate it.").into();
            }
            Err(error) => self.status = error.to_string(),
        }
        cx.notify();
    }

    pub fn record_registered_shortcut(&mut self, shortcut: &str, cx: &mut Context<Self>) {
        if let Some(mode) = self.recording {
            self.capture_shortcut(mode, shortcut.to_owned(), cx);
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn smoke_begin_shortcut_recording(
        &mut self,
        saved: &Settings,
        target: TranslationMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.shortcuts = ShortcutDraft::new(saved);
        self.category = Category::Shortcuts;
        self.recording = Some(target);
        self.focus.focus(window, cx);
        cx.notify();
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn smoke_shortcut_recorded(&self) -> bool {
        self.recording.is_none()
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn smoke_verify_shortcut_reassignment(
        &mut self,
        saved: &Settings,
        source: TranslationMode,
        target: TranslationMode,
        cx: &mut Context<Self>,
    ) {
        assert_eq!(self.shortcuts.values[target.index()], "Ctrl+KeyM");
        assert!(self.shortcuts.values[source.index()].is_empty());
        assert!(self.shortcuts.warning().is_some());
        assert!(!self.shortcuts.complete());
        // Move the registered combination back without saving or reopening, then
        // reassign the now-empty action and validate the final form.
        self.recording = Some(source);
        self.record_registered_shortcut("Ctrl+KeyM", cx);
        assert!(self.shortcuts.values[target.index()].is_empty());
        self.recording = Some(target);
        self.record_registered_shortcut(saved.shortcuts()[target.index()], cx);
        assert!(self.shortcuts.complete());
        assert!(self.shortcuts.warning().is_none());
        assert_eq!(
            self.shortcuts.values.each_ref().map(String::as_str),
            saved.shortcuts()
        );
    }
}

impl Drop for SettingsView {
    fn drop(&mut self) {
        if let Some(task) = self.pending.take() {
            task.abort();
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let logo_size = ((if self.onboarding_step.is_some() { 40. } else { 36. })
            * window.scale_factor())
        .round()
        .max(1.) as u32;
        if self.logo_size != logo_size {
            self.logo_light = resized_logo(LOGO_LIGHT, logo_size);
            self.logo_dark = resized_logo(LOGO_DARK, logo_size);
            self.logo_size = logo_size;
        }
        let mut sidebar = v_flex()
            .w(px(200.))
            .flex_shrink_0()
            .p_3()
            .gap_1()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                div().px_3().pt_3().child(
                    img(if cx.theme().is_dark() {
                        self.logo_dark.clone()
                    } else {
                        self.logo_light.clone()
                    })
                    .size(px(36.))
                    .object_fit(ObjectFit::Contain),
                ),
            )
            .child(
                div()
                    .px_3()
                    .py_4()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(t("SETTINGS")),
            );
        for category in Category::ALL {
            let selected = self.category == category;
            sidebar = sidebar.child(
                Button::new(("category-nav", category as usize))
                    .ghost()
                    .w_full()
                    .justify_start()
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_start()
                            .gap_2()
                            .child(Icon::new(category.icon()).size_4().flex_shrink_0())
                            .child(category.title()),
                    )
                    .selected(selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.category = category;
                        if this.recording.take().is_some() {
                            this.status = t("Shortcut capture cancelled.").into();
                        }
                        cx.notify();
                    })),
            );
        }
        sidebar = sidebar.child(div().flex_1()).child(
            div()
                .px_3()
                .py_2()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(concat!("v", env!("CARGO_PKG_VERSION"))),
        );
        let mut fields = v_flex().gap_4();
        match self.category {
            Category::General => {
                fields = fields
                    .child(section("Appearance", "The theme applies to every application window.", cx))
                    .child(field("Theme", Select::new(&self.theme).w_full()))
                    .child(hint("Saved immediately. System follows the desktop theme.", cx))
                    .child(field("Interface language", Select::new(&self.language).w_full()))
                    .child(hint("Saved immediately. System uses French for a French system locale, English otherwise.", cx))
                    .child(section("Startup", "Find the application in the notification area.", cx))
                    .child(Checkbox::new("launch-at-startup").label(t("Launch at sign-in")).checked(self.launch_at_startup)
                        .on_click(cx.listener(|this, checked, _, cx| { this.launch_at_startup = *checked; cx.notify(); })))
                    .child(hint("Save to apply this option. The application starts in the tray when you sign in.", cx))
                    .child(hint("Closing windows leaves the application in the tray. To exit: tray → Quit.", cx));
            }
            Category::Provider => {
                fields = fields
                    .child(field("Provider", Select::new(&self.provider).w_full()))
                    .child(field("Base URL", Input::new(&self.base_url)))
                    .child(hint("Include /v1, without /chat/completions.", cx))
                    .child(field("Model", Input::new(&self.model)))
                    .child(field("API key", Input::new(&self.api_key).mask_toggle()))
                    .child(hint(
                        "Stored in the system keyring. Optional for a local server.",
                        cx,
                    ))
                    .child(
                        Button::new("test")
                            .label(t(if self.testing {
                                "Testing…"
                            } else {
                                "Test connection"
                            }))
                            .disabled(self.testing)
                            .on_click(cx.listener(|this, _, window, cx| this.test(window, cx))),
                    );
            }
            Category::Translation => {
                fields = fields
                    .child(section("Default languages", "You can change these languages in the preview.", cx))
                    .child(field("Source language", Select::new(&self.source).w_full()))
                    .child(field("Target language", Select::new(&self.target).w_full()))
                    .child(section("With preview", "Review or edit the translation before replacing the text.", cx))
                    .child(self.shortcut(TranslationMode::Preview, cx))
                    .child(section("Quick Translate", "Translates and replaces the selection in the background using the saved languages and provider.", cx))
                    .child(self.shortcut(TranslationMode::Quick, cx));
                if self.onboarding_step.is_some() {
                    fields = v_flex().gap_4()
                        .child(field("Source language", Select::new(&self.source).w_full()))
                        .child(field("Target language", Select::new(&self.target).w_full()))
                        .child(hint("Interface language does not affect translation languages.", cx))
                        .child(hint("Proofreading keeps the text's language. Faithful mode preserves tone and wording; other modes adjust the style without changing the meaning.", cx))
                        .child(field("With preview", style_buttons("onboarding-style", self.style)
                            .on_click(cx.listener(|this, indices: &Vec<usize>, _, cx| {
                                if let Some(style) = indices.first().and_then(|index| CorrectionStyle::ALL.get(*index)).copied() {
                                    this.style = style;
                                    cx.notify();
                                }
                            }))))
                        .child(field("Quick Check", style_buttons("onboarding-quick-style", self.quick_style)
                            .on_click(cx.listener(|this, indices: &Vec<usize>, _, cx| {
                                if let Some(style) = indices.first().and_then(|index| CorrectionStyle::ALL.get(*index)).copied() {
                                    this.quick_style = style;
                                    cx.notify();
                                }
                            }))));
                }
            }
            Category::Correction => {
                fields = fields
                    .child(hint("Proofreading keeps the text's language. Faithful mode preserves tone and wording; other modes adjust the style without changing the meaning.", cx))
                    .child(section("With preview", "Review or edit the correction before replacing the text.", cx))
                    .child(field("Default mode", style_buttons("default-correction-modes", self.style)
                        .on_click(cx.listener(|this, indices: &Vec<usize>, _, cx| {
                            if let Some(style) = indices.first().and_then(|index| CorrectionStyle::ALL.get(*index)).copied() {
                                this.style = style;
                                cx.notify();
                            }
                        }))))
                    .child(self.shortcut(TranslationMode::CorrectionPreview, cx))
                    .child(section("Quick Check", "Proofreads and replaces the selection directly in the background.", cx))
                    .child(field("Default mode", style_buttons("quick-correction-modes", self.quick_style)
                        .on_click(cx.listener(|this, indices: &Vec<usize>, _, cx| {
                            if let Some(style) = indices.first().and_then(|index| CorrectionStyle::ALL.get(*index)).copied() {
                                this.quick_style = style;
                                cx.notify();
                            }
                        }))))
                    .child(self.shortcut(TranslationMode::CorrectionQuick, cx));
            }
            Category::Shortcuts => {
                fields = fields
                    .child(section("Translation", "Preview or direct replacement with Quick Translate.", cx))
                    .child(field("With preview", self.shortcut(TranslationMode::Preview, cx)))
                    .child(field("Quick Translate", self.shortcut(TranslationMode::Quick, cx)))
                    .child(section("Proofreading", "Preview or direct replacement with Quick Check.", cx))
                    .child(field("With preview", self.shortcut(TranslationMode::CorrectionPreview, cx)))
                    .child(field("Quick Check", self.shortcut(TranslationMode::CorrectionQuick, cx)))
                    .child(hint("Shortcuts are shared with Translation and Proofreading. They must be distinct. Save to activate them.", cx));
            }
        }
        if self.onboarding_step.is_some() {
            return self.render_onboarding(fields, cx).into_any_element();
        }
        h_flex()
            .size_full()
            .items_stretch()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event, _, cx| this.record(event, cx)))
            .child(sidebar)
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .p_6()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_2()
                            .pb_4()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.category.title()),
                            )
                            .child(hint(self.category.description(), cx)),
                    )
                    .child(
                        v_flex()
                            .id(("category-content", self.category as usize))
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .child(fields),
                    )
                    .child(
                        v_flex()
                            .flex_shrink_0()
                            .pt_4()
                            .gap_3()
                            .border_t_1()
                            .border_color(cx.theme().border)
                            .children(self.shortcuts.warning().map(|warning| {
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().warning)
                                    .child(warning)
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .max_h(px(80.))
                                    .id("settings-status")
                                    .overflow_y_scroll()
                                    .child(crate::i18n::localize_message(&self.status)),
                            )
                            .child(
                                h_flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        Button::new("close")
                                            .ghost()
                                            .label(t("Close"))
                                            .on_click(|_, window, _| window.remove_window()),
                                    )
                                    .child(
                                        Button::new("save")
                                            .primary()
                                            .label(t("Save"))
                                            .disabled(
                                                self.recording.is_some()
                                                    || !self.shortcuts.complete(),
                                            )
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.save(cx);
                                            })),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl SettingsView {
    fn advance_onboarding(&mut self, cx: &mut Context<Self>) {
        let step = self.onboarding_step.unwrap_or(0);
        if (1..=3).contains(&step)
            && let Err(error) = self.values(cx)
        {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        if step == 4 && !self.save(cx) {
            return;
        }
        self.set_onboarding_step(step + 1, cx);
    }

    fn set_onboarding_step(&mut self, step: usize, cx: &mut Context<Self>) {
        self.cancel_test();
        self.status.clear();
        self.onboarding_step = Some(step);
        self.category = match step {
            1 => Category::Provider,
            2 => Category::Translation,
            3 => Category::Shortcuts,
            _ => Category::General,
        };
        cx.notify();
    }

    pub fn smoke_onboarding_step(&mut self, step: usize, cx: &mut Context<Self>) {
        assert!(step <= 5, "Smoke diagnostics must never save setup");
        if step == 5 {
            assert_eq!(self.onboarding_step, Some(4));
            // Exercise backward navigation while retaining unsaved form values.
            let model = self.model.read(cx).value();
            self.set_onboarding_step(0, cx);
            assert_eq!(self.model.read(cx).value(), model);
            return;
        }
        if step == 0 {
            assert_eq!(self.onboarding_step, Some(0));
        } else {
            assert_eq!(self.onboarding_step, Some(step - 1));
            self.advance_onboarding(cx);
            assert_eq!(self.onboarding_step, Some(step));
        }
    }

    fn render_onboarding(&self, fields: Div, cx: &mut Context<Self>) -> Div {
        let step = self.onboarding_step.unwrap_or(0);
        let background = cx.theme().background;
        let mut transparent = background;
        transparent.a = 0.;
        let title = match step {
            0 => "Welcome to Emendia",
            1 => "Connect your AI provider",
            2 => "Make Emendia yours",
            3 => "Choose your shortcuts",
            4 => "Ready to start",
            _ => "You're all set!",
        };
        let description = match step {
            0 => "Translate, proofread and refine your text without leaving your application.",
            1 => {
                "Choose a cloud or local model. Testing the connection is recommended but optional."
            }
            2 => "Choose your default translation languages and proofreading styles.",
            3 => {
                "Keep the defaults or record your own shortcuts. Preview lets you review; quick actions replace text directly."
            }
            4 => "Review your configuration, then save it to activate Emendia.",
            _ => {
                "Open an editor, select text, press your translation shortcut and release its keys. Review the suggestion, then choose Replace or Copy."
            }
        };
        let heading = v_flex()
            .gap_2()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(t(title)),
            )
            .child(hint(description, cx));
        let mut content = v_flex().gap_5().w_full().max_w(px(660.)).mx_auto();
        let mut fixed_header = None;
        if step == 0 {
            content = content.text_center().child(heading).child(
                h_flex()
                    .gap_4()
                    .items_stretch()
                    .child(div().flex_1().min_w(px(0.)).text_left().child(field(
                        "Interface language",
                        Select::new(&self.language).w_full(),
                    )))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_left()
                            .child(field("Theme", Select::new(&self.theme).w_full())),
                    ),
            );
        } else {
            fixed_header = Some(
                v_flex()
                    .gap_5()
                    .w_full()
                    .max_w(px(660.))
                    .mx_auto()
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(
                                img(if cx.theme().is_dark() {
                                    self.logo_dark.clone()
                                } else {
                                    self.logo_light.clone()
                                })
                                .size(px(40.))
                                .object_fit(ObjectFit::Contain),
                            )
                            .child(div().text_sm().child(format!(
                                "{} {} {} 5",
                                t("Step"),
                                step.min(4) + 1,
                                t("of")
                            ))),
                    )
                    .child(heading),
            );
            if step < 4 {
                content = content.child(fields);
                if step == 1 {
                    content = content.child(hint("For local providers, start the server and load the model before testing. OpenAI requires API credits, separate from a ChatGPT subscription.", cx));
                }
            } else if step == 4 {
                content = content
                    .child(
                        v_flex()
                            .gap_2()
                            .p_4()
                            .rounded_lg()
                            .bg(cx.theme().secondary)
                            .child(format!(
                                "{}: {}",
                                t("Provider"),
                                self.provider
                                    .read(cx)
                                    .selected_value()
                                    .cloned()
                                    .unwrap_or_default()
                            ))
                            .child(format!("{}: {}", t("Model"), self.model.read(cx).value()))
                            .child(format!(
                                "{}: {}",
                                t("Base URL"),
                                self.base_url.read(cx).value()
                            ))
                            .child(format!(
                                "{}: {}",
                                t("Source language"),
                                self.source
                                    .read(cx)
                                    .selected_value()
                                    .cloned()
                                    .unwrap_or_default()
                            ))
                            .child(format!(
                                "{}: {}",
                                t("Target language"),
                                self.target
                                    .read(cx)
                                    .selected_value()
                                    .cloned()
                                    .unwrap_or_default()
                            ))
                            .child(format!(
                                "{}: {} / {}",
                                t("Proofreading"),
                                self.style.label(),
                                self.quick_style.label()
                            ))
                            .child(format!(
                                "{}: {} · {} · {} · {}",
                                t("Shortcuts"),
                                self.shortcuts.values[0],
                                self.shortcuts.values[1],
                                self.shortcuts.values[2],
                                self.shortcuts.values[3]
                            )),
                    )
                    .child(
                        Checkbox::new("onboarding-startup")
                            .label(t("Launch at sign-in"))
                            .checked(self.launch_at_startup)
                            .on_click(cx.listener(|this, checked, _, cx| {
                                this.launch_at_startup = *checked;
                                cx.notify();
                            })),
                    );
            } else {
                content = content
                    .child(
                        div()
                            .p_4()
                            .rounded_lg()
                            .bg(cx.theme().secondary)
                            .child(self.shortcuts.values[0].clone()),
                    )
                    .child(hint(
                        "Closing windows leaves the application in the tray. To exit: tray → Quit.",
                        cx,
                    ));
            }
        }
        let mut navigation = h_flex().gap_3().justify_center();
        if step > 0 && step < 5 {
            navigation = navigation.child(
                Button::new("onboarding-back")
                    .ghost()
                    .label(t("Back"))
                    .disabled(self.recording.is_some())
                    .on_click(cx.listener(|this, _, _, cx| {
                        let step = this.onboarding_step.unwrap_or(1).saturating_sub(1);
                        this.set_onboarding_step(step, cx);
                    })),
            );
        }
        navigation = navigation.child(
            Button::new("onboarding-next")
                .primary()
                .label(t(match step {
                    0 => "Get started",
                    4 => "Finish setup",
                    5 => "Let's go",
                    _ => "Next",
                }))
                .disabled(self.recording.is_some())
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.onboarding_step == Some(5) {
                        window.remove_window();
                    } else {
                        this.advance_onboarding(cx);
                    }
                })),
        );
        let mut dots = h_flex().gap_2().justify_center();
        for index in 0..5 {
            dots = dots.child(
                div()
                    .size(px(6.))
                    .rounded_full()
                    .bg(if index == step.min(4) {
                        cx.theme().primary
                    } else {
                        cx.theme().border
                    }),
            );
        }
        let footer = v_flex()
            .gap_3()
            .flex_shrink_0()
            .px_6()
            .pb_5()
            .child(
                div()
                    .id("onboarding-status")
                    .max_h(px(60.))
                    .overflow_y_scroll()
                    .text_sm()
                    .text_center()
                    .child(crate::i18n::localize_message(&self.status)),
            )
            .children(self.shortcuts.warning().map(|warning| {
                div()
                    .text_sm()
                    .text_center()
                    .text_color(cx.theme().warning)
                    .child(warning)
            }))
            .child(navigation)
            .child(dots);
        let mut root = v_flex()
            .relative()
            .size_full()
            .bg(background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event, _, cx| this.record(event, cx)));
        if step == 0 {
            root = root.child(
                div()
                    .relative()
                    .w_full()
                    .h(px(ONBOARDING_HERO_HEIGHT))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(
                        img(self.hero.clone())
                            .absolute()
                            .top_0()
                            .left_0()
                            .w_full()
                            .h(px(ONBOARDING_HERO_HEIGHT))
                            .object_fit(ObjectFit::Cover),
                    )
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .w_full()
                            .h(px(140.))
                            .bg(linear_gradient(
                                180.,
                                linear_color_stop(transparent, 0.),
                                linear_color_stop(background, 1.),
                            )),
                    ),
            );
        } else if let Some(header) = fixed_header {
            root = root.child(
                div()
                    .flex_shrink_0()
                    .px_8()
                    .pt(px(ONBOARDING_TITLE_BAR_HEIGHT + 20.))
                    .pb_3()
                    .child(header),
            );
        }
        root.child(
            div()
                .relative()
                .flex_1()
                .min_h(px(0.))
                .child(
                    div()
                        .id("onboarding-content")
                        .size_full()
                        .overflow_y_scroll()
                        .px_8()
                        .pt_5()
                        .pb_10()
                        .child(content),
                )
                .child(div().absolute().bottom_0().left_0().w_full().h(px(32.)).bg(
                    linear_gradient(
                        180.,
                        linear_color_stop(transparent, 0.),
                        linear_color_stop(background, 1.),
                    ),
                )),
        )
        .child(footer)
        // Overlay the native window-control regions without reserving space above the hero.
        .when(cfg!(target_os = "windows"), |root| {
            root.child(
                div().absolute().top_0().left_0().w_full().child(
                    TitleBar::new()
                        .h(px(ONBOARDING_TITLE_BAR_HEIGHT))
                        .bg(transparent)
                        .when(step == 0, |bar| {
                            // Keep the controls legible over the photo in both themes.
                            bar.bg(linear_gradient(
                                90.,
                                linear_color_stop(transparent, 0.78),
                                linear_color_stop(background, 0.92),
                            ))
                        })
                        .border_0(),
                ),
            )
        })
    }

    fn localize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::ui::refresh_language(&self.source, true, window, cx);
        crate::ui::refresh_language(&self.target, false, window, cx);
        self.api_key.update(cx, |state, cx| {
            state.set_placeholder(t("Optional for a local server"), window, cx)
        });
        for (select, values) in [
            (
                &self.theme,
                ThemePreference::ALL
                    .into_iter()
                    .map(|v| v.label().to_owned())
                    .collect::<Vec<_>>(),
            ),
            (
                &self.language,
                UiLanguage::ALL
                    .into_iter()
                    .map(|v| v.label().to_owned())
                    .collect::<Vec<_>>(),
            ),
            (
                &self.provider,
                PROVIDERS
                    .iter()
                    .map(|v| t(v).to_owned())
                    .collect::<Vec<_>>(),
            ),
        ] {
            let index = select.read(cx).selected_index(cx);
            select.update(cx, |state, cx| {
                state.set_items(values, window, cx);
                state.set_selected_index(index, window, cx);
            });
        }
        window.set_window_title(t(if self.onboarding_step.is_some() {
            "Emendia — Welcome"
        } else {
            "Emendia — Settings"
        }));
    }

    fn shortcut(&self, mode: TranslationMode, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, value) = match mode {
            TranslationMode::Preview => ("record", &self.shortcuts.values[0]),
            TranslationMode::Quick => ("record-quick", &self.shortcuts.values[1]),
            TranslationMode::CorrectionPreview => ("record-correction", &self.shortcuts.values[2]),
            TranslationMode::CorrectionQuick => {
                ("record-quick-correction", &self.shortcuts.values[3])
            }
        };
        h_flex()
            .gap_3()
            .justify_between()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(cx.theme().secondary)
                    .text_sm()
                    .when(value.is_empty(), |row| row.text_color(cx.theme().warning))
                    .child(if value.is_empty() {
                        t("Unassigned — choose a shortcut").to_owned()
                    } else {
                        value.clone()
                    }),
            )
            .child(
                Button::new(id)
                    .label(t(if self.recording == Some(mode) {
                        "Press the shortcut…"
                    } else {
                        "Change shortcut"
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.recording = Some(mode);
                        this.focus.focus(window, cx);
                        this.status = t("Press the desired combination (Escape to cancel).").into();
                        cx.notify();
                    })),
            )
    }
}

fn field(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_2()
        .child(div().text_sm().child(t(label)))
        .child(control)
}

fn hint(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(t(text))
}

fn section(title: &'static str, description: &'static str, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1()
        .pt_2()
        .child(div().font_weight(FontWeight::SEMIBOLD).child(t(title)))
        .child(hint(description, cx))
}

#[cfg(test)]
mod tests {
    use super::{Category, ShortcutDraft};
    use crate::{
        platform::hotkey::{self, TranslationMode},
        settings::Settings,
    };
    use gpui_kit::AssetSource;

    #[test]
    fn category_icons_are_available() {
        let assets = gpui_kit::assets::AllAssets;
        for category in Category::ALL {
            let path = category.icon().path();
            assert!(
                assets
                    .load(path.as_ref())
                    .is_ok_and(|asset| asset.is_some()),
                "Missing category icon: {path}"
            );
        }
    }

    #[test]
    fn shortcuts_can_move_between_every_pair_of_actions_without_saving() {
        for source in TranslationMode::ALL {
            for target in TranslationMode::ALL {
                if source == target {
                    continue;
                }
                let mut draft = ShortcutDraft::new(&Settings::default());
                draft.assign(source, "Ctrl+KeyM".into()).unwrap();
                draft.assign(target, "Ctrl+KeyM".into()).unwrap();
                assert!(draft.values[source.index()].is_empty());
                assert_eq!(draft.values[target.index()], "Ctrl+KeyM");
                assert!(!draft.complete());
                let reassignment = draft.reassignment.as_ref().unwrap();
                assert_eq!(reassignment.displaced, [source]);
                assert_eq!(reassignment.target, target);
                assert!(draft.warning().is_some());
                assert!(
                    hotkey::parse_shortcuts(draft.values.each_ref().map(String::as_str)).is_err()
                );
                // Swap ownership back in the same unsaved form, then repair the
                // displaced action and validate what will actually be persisted.
                draft.assign(source, "Ctrl+KeyM".into()).unwrap();
                assert!(draft.values[target.index()].is_empty());
                draft.assign(target, "Ctrl+Alt+KeyN".into()).unwrap();
                assert!(draft.complete());
                assert!(draft.warning().is_none());
                assert!(
                    hotkey::parse_shortcuts(draft.values.each_ref().map(String::as_str)).is_ok()
                );
            }
        }
    }

    #[test]
    fn shortcut_aliases_invalid_input_and_same_action_are_handled_consistently() {
        let mut draft = ShortcutDraft::new(&Settings::default());
        draft
            .assign(TranslationMode::Preview, "Ctrl+Alt+KeyM".into())
            .unwrap();
        draft
            .assign(TranslationMode::Quick, "Alt+Control+KeyM".into())
            .unwrap();
        assert!(draft.values[0].is_empty());
        let values = draft.values.clone();
        assert!(
            draft
                .assign(TranslationMode::CorrectionQuick, "Shift+KeyM".into())
                .is_err()
        );
        assert_eq!(
            draft.values, values,
            "Invalid input must not displace any action"
        );
        draft
            .assign(TranslationMode::Quick, "Alt+Control+KeyM".into())
            .unwrap();
        assert_eq!(
            draft.values, values,
            "Recording the current value must not clear itself"
        );
        assert!(
            draft.warning().is_some(),
            "The unassigned action must still be indicated"
        );
    }

    #[test]
    fn repeated_moves_and_legacy_duplicates_clear_all_previous_owners() {
        let mut draft = ShortcutDraft::new(&Settings::default());
        draft.values = [
            "Ctrl+KeyM".into(),
            "Control+KeyM".into(),
            "Ctrl+F11".into(),
            "Ctrl+Shift+F11".into(),
        ];
        draft
            .assign(TranslationMode::CorrectionPreview, "Ctrl+KeyM".into())
            .unwrap();
        assert!(draft.values[0].is_empty() && draft.values[1].is_empty());
        assert_eq!(
            draft.reassignment.as_ref().unwrap().displaced,
            [TranslationMode::Preview, TranslationMode::Quick]
        );
        for target in TranslationMode::ALL.into_iter().cycle().take(12) {
            draft.assign(target, "Ctrl+KeyM".into()).unwrap();
            assert_eq!(
                draft
                    .values
                    .iter()
                    .filter(|value| value.as_str() == "Ctrl+KeyM")
                    .count(),
                1
            );
            assert_eq!(draft.values[target.index()], "Ctrl+KeyM");
        }
    }
}
