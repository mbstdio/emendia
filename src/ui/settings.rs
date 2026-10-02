use crate::{
    app::Controller,
    platform::hotkey::{self, TranslationMode},
    settings::{self, CorrectionStyle, Settings, ThemePreference},
    ui::{LanguageSelect, language_select, style_buttons, text_input},
};
use anyhow::{Context as _, Result};
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

const PROVIDERS: &[&str] = &["OpenAI", "LM Studio", "Ollama", "Personnalisé"];

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
            Self::General => "Général",
            Self::Provider => "Provider IA",
            Self::Translation => "Traduction",
            Self::Correction => "Correction",
            Self::Shortcuts => "Raccourcis",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::General => "Personnalise l’apparence et le démarrage de l’application.",
            Self::Provider => "Configure le modèle utilisé pour traduire et corriger tes textes.",
            Self::Translation => "Choisis les langues et les raccourcis de traduction.",
            Self::Correction => "Adapte le style de correction et ses raccourcis.",
            Self::Shortcuts => "Retrouve tous tes raccourcis globaux au même endroit.",
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
    hotkey: String,
    quick_hotkey: String,
    correction_hotkey: String,
    quick_correction_hotkey: String,
    style: CorrectionStyle,
    quick_style: CorrectionStyle,
    launch_at_startup: bool,
    theme: LanguageSelect,
    theme_preference: ThemePreference,
    category: Category,
    recording: Option<TranslationMode>,
    focus: FocusHandle,
    pub status: String,
    testing: bool,
    pending: Option<AbortHandle>,
    test_version: u64,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(
        settings: Settings,
        controller: WeakEntity<Controller>,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
                    .map(|p| (*p).into())
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
                .placeholder("Facultative pour un serveur local");
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
        let mut subscriptions = vec![
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
                                this.status = "Thème appliqué et enregistré.".into();
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
                            "OpenAI" => Some(("https://api.openai.com/v1", "gpt-4.1-mini")),
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
                                "Renseigne le nom exact du modèle disponible sur ce provider."
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
        Self {
            controller,
            provider,
            base_url,
            model,
            api_key,
            source,
            target,
            hotkey: settings.hotkey,
            quick_hotkey: settings.quick_hotkey,
            correction_hotkey: settings.correction_hotkey,
            quick_correction_hotkey: settings.quick_correction_hotkey,
            style,
            quick_style,
            launch_at_startup: settings.launch_at_startup,
            theme,
            theme_preference: settings.theme,
            category: Category::General,
            recording: None,
            focus: cx.focus_handle(),
            status: error.or(key_error).unwrap_or_else(|| {
                "Configure le provider, puis teste la connexion et enregistre.".into()
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
            self.status = "Configuration modifiée ; relance le test de connexion.".into();
        }
        self.testing = false;
    }

    fn values(&self, cx: &App) -> Result<(Settings, String)> {
        let settings = Settings {
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
                .context("Choisis la langue source")?
                .clone(),
            target_language: self
                .target
                .read(cx)
                .selected_value()
                .context("Choisis la langue cible")?
                .clone(),
            hotkey: self.hotkey.clone(),
            quick_hotkey: self.quick_hotkey.clone(),
            correction_hotkey: self.correction_hotkey.clone(),
            quick_correction_hotkey: self.quick_correction_hotkey.clone(),
            correction_style: self.style,
            quick_correction_style: self.quick_style,
            launch_at_startup: self.launch_at_startup,
            theme: self.theme_preference,
        };
        settings.validate()?;
        settings.validate_hotkeys()?;
        Ok((settings, self.api_key.read(cx).value().trim().into()))
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let result = self.values(cx).and_then(|(settings, key)| {
            self.controller
                .update(cx, |app, _| app.save_settings(settings, &key))?
        });
        self.status = match result {
            Ok(()) => {
                "Paramètres enregistrés. Les raccourcis sont actifs ; tu peux fermer cette fenêtre."
                    .into()
            }
            Err(error) => error.to_string(),
        };
        cx.notify();
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
        self.status = "Test de connexion en cours…".into();
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                if this.test_version != version {
                    return;
                }
                this.pending = None;
                this.testing = false;
                this.status = match result {
                    Ok(Ok(text)) => format!("Connexion réussie : {text}"),
                    Ok(Err(error)) => error.to_string(),
                    Err(_) => "Test interrompu.".into(),
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
            self.status = "Utilise une lettre, un chiffre ou F1–F24 avec Ctrl, Alt ou Win.".into();
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
        match hotkey::parse(&candidate) {
            Ok(_) => {
                match mode {
                    TranslationMode::Preview => self.hotkey = candidate,
                    TranslationMode::Quick => self.quick_hotkey = candidate,
                    TranslationMode::CorrectionPreview => self.correction_hotkey = candidate,
                    TranslationMode::CorrectionQuick => self.quick_correction_hotkey = candidate,
                }
                self.recording = None;
                self.status = "Raccourci capturé. Enregistre pour l’activer.".into();
            }
            Err(error) => self.status = error.to_string(),
        }
        cx.notify();
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sidebar = v_flex()
            .w(px(200.))
            .flex_shrink_0()
            .p_3()
            .gap_1()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .px_3()
                    .py_4()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("PARAMÈTRES"),
            );
        for category in Category::ALL {
            let selected = self.category == category;
            sidebar = sidebar.child(
                Button::new(("category-nav", category as usize))
                    .ghost()
                    .w_full()
                    .justify_start()
                    .icon(Icon::new(category.icon()).size_4())
                    .label(category.title())
                    .selected(selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.category = category;
                        if this.recording.take().is_some() {
                            this.status = "Capture du raccourci annulée.".into();
                        }
                        cx.notify();
                    })),
            );
        }
        let mut fields = v_flex().gap_4();
        match self.category {
            Category::General => {
                fields = fields
                    .child(section("Apparence", "Le thème s’applique à toutes les fenêtres de l’application.", cx))
                    .child(field("Thème", Select::new(&self.theme).w_full()))
                    .child(hint("Le choix est enregistré immédiatement. Système suit le thème de Windows.", cx))
                    .child(section("Démarrage", "Retrouve l’application dans la zone de notification.", cx))
                    .child(Checkbox::new("launch-at-startup").label("Lancer au démarrage de Windows").checked(self.launch_at_startup)
                        .on_click(cx.listener(|this, checked, _, cx| { this.launch_at_startup = *checked; cx.notify(); })))
                    .child(hint("Enregistre pour appliquer cette option. L’application démarre dans le tray à l’ouverture de ta session.", cx))
                    .child(hint("Fermer les fenêtres laisse l’application dans le tray. Pour arrêter : tray → Quitter.", cx));
            }
            Category::Provider => {
                fields = fields
                    .child(field("Provider", Select::new(&self.provider).w_full()))
                    .child(field("URL de base", Input::new(&self.base_url)))
                    .child(hint("Avec /v1, sans /chat/completions.", cx))
                    .child(field("Modèle", Input::new(&self.model)))
                    .child(field("Clé API", Input::new(&self.api_key)))
                    .child(hint("Enregistrée dans le gestionnaire d’identifiants Windows. Facultative pour un serveur local.", cx))
                    .child(Button::new("test").label(if self.testing { "Test en cours…" } else { "Tester la connexion" }).disabled(self.testing)
                        .on_click(cx.listener(|this, _, window, cx| this.test(window, cx))));
            }
            Category::Translation => {
                fields = fields
                    .child(section("Langues par défaut", "Ces langues restent modifiables dans l’aperçu.", cx))
                    .child(field("Langue source", Select::new(&self.source).w_full()))
                    .child(field("Langue cible", Select::new(&self.target).w_full()))
                    .child(section("Avec aperçu", "Vérifie ou édite la traduction avant de remplacer le texte.", cx))
                    .child(self.shortcut(TranslationMode::Preview, cx))
                    .child(section("Quick Translate", "Traduit et remplace directement la sélection en arrière-plan, avec les langues et le provider enregistrés.", cx))
                    .child(self.shortcut(TranslationMode::Quick, cx));
            }
            Category::Correction => {
                fields = fields
                    .child(hint("La correction conserve la langue du texte. Le mode fidèle préserve le ton et les formulations ; les autres modes adaptent le style sans changer le sens.", cx))
                    .child(section("Avec aperçu", "Vérifie ou édite la correction avant de remplacer le texte.", cx))
                    .child(field("Mode par défaut", style_buttons("default-correction-modes", self.style)
                        .on_click(cx.listener(|this, indices: &Vec<usize>, _, cx| {
                            if let Some(style) = indices.first().and_then(|index| CorrectionStyle::ALL.get(*index)).copied() {
                                this.style = style;
                                cx.notify();
                            }
                        }))))
                    .child(self.shortcut(TranslationMode::CorrectionPreview, cx))
                    .child(section("Quick Check", "Corrige et remplace directement la sélection en arrière-plan.", cx))
                    .child(field("Mode par défaut", style_buttons("quick-correction-modes", self.quick_style)
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
                    .child(section("Traduction", "Avec aperçu ou remplacement direct avec Quick Translate.", cx))
                    .child(field("Avec aperçu", self.shortcut(TranslationMode::Preview, cx)))
                    .child(field("Quick Translate", self.shortcut(TranslationMode::Quick, cx)))
                    .child(section("Correction", "Avec aperçu ou remplacement direct avec Quick Check.", cx))
                    .child(field("Avec aperçu", self.shortcut(TranslationMode::CorrectionPreview, cx)))
                    .child(field("Quick Check", self.shortcut(TranslationMode::CorrectionQuick, cx)))
                    .child(hint("Les raccourcis sont partagés avec les rubriques Traduction et Correction. Ils doivent être distincts. Enregistre pour les activer.", cx));
            }
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
                            .child(
                                div()
                                    .text_sm()
                                    .max_h(px(80.))
                                    .id("settings-status")
                                    .overflow_y_scroll()
                                    .child(self.status.clone()),
                            )
                            .child(
                                h_flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        Button::new("close")
                                            .ghost()
                                            .label("Fermer")
                                            .on_click(|_, window, _| window.remove_window()),
                                    )
                                    .child(
                                        Button::new("save")
                                            .primary()
                                            .label("Enregistrer")
                                            .disabled(self.recording.is_some())
                                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                                    ),
                            ),
                    ),
            )
    }
}

impl SettingsView {
    fn shortcut(&self, mode: TranslationMode, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, value) = match mode {
            TranslationMode::Preview => ("record", &self.hotkey),
            TranslationMode::Quick => ("record-quick", &self.quick_hotkey),
            TranslationMode::CorrectionPreview => ("record-correction", &self.correction_hotkey),
            TranslationMode::CorrectionQuick => {
                ("record-quick-correction", &self.quick_correction_hotkey)
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
                    .child(value.clone()),
            )
            .child(
                Button::new(id)
                    .label(if self.recording == Some(mode) {
                        "Appuie sur le raccourci…"
                    } else {
                        "Changer le raccourci"
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.recording = Some(mode);
                        this.focus.focus(window, cx);
                        this.status =
                            "Appuie sur la combinaison souhaitée (Échap pour annuler).".into();
                        cx.notify();
                    })),
            )
    }
}

fn field(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_2()
        .child(div().text_sm().child(label))
        .child(control)
}

fn hint(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

fn section(title: &'static str, description: &'static str, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1()
        .pt_2()
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
        .child(hint(description, cx))
}
