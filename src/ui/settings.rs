use crate::{
    app::Controller,
    platform::hotkey,
    settings::{self, Settings},
    ui::{LanguageSelect, language_select, text_input},
};
use anyhow::{Context as _, Result};
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        input::{Input, InputEvent, InputState},
        select::{Select, SelectEvent, SelectState},
        *,
    },
    *,
};
use tokio::task::AbortHandle;

const PROVIDERS: &[&str] = &["OpenAI", "LM Studio", "Ollama", "Personnalisé"];

pub struct SettingsView {
    controller: WeakEntity<Controller>,
    provider: Entity<SelectState<Vec<String>>>,
    base_url: Entity<InputState>,
    model: Entity<InputState>,
    api_key: Entity<InputState>,
    source: LanguageSelect,
    target: LanguageSelect,
    hotkey: String,
    recording: bool,
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
        let mut subscriptions = vec![
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
            recording: false,
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
        };
        settings.validate()?;
        hotkey::parse(&settings.hotkey)?;
        Ok((settings, self.api_key.read(cx).value().trim().into()))
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let result = self.values(cx).and_then(|(settings, key)| {
            self.controller
                .update(cx, |app, _| app.save_settings(settings, &key))?
        });
        self.status = match result {
            Ok(()) => {
                "Paramètres enregistrés. La hotkey est active ; tu peux fermer cette fenêtre."
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
        if !self.recording {
            return;
        }
        cx.stop_propagation();
        let stroke = &event.keystroke;
        if stroke.key == "escape" {
            self.recording = false;
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
                self.hotkey = candidate;
                self.recording = false;
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
        v_flex().size_full().bg(cx.theme().background).text_color(cx.theme().foreground).p_5().gap_4()
            .child(div().text_xl().font_weight(FontWeight::BOLD).child("Translation Tool"))
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Sélectionne un texte → hotkey → vérifie la traduction → remplace."))
            .child(v_flex().id("settings-fields").flex_1().min_h(px(0.)).overflow_y_scroll().gap_3()
                .child(div().text_sm().child("Provider"))
                .child(Select::new(&self.provider).w_full())
                .child(div().text_sm().child("URL de base (avec /v1, sans /chat/completions)"))
                .child(Input::new(&self.base_url))
                .child(div().text_sm().child("Modèle"))
                .child(Input::new(&self.model))
                .child(div().text_sm().child("Clé API — enregistrée dans le gestionnaire d’identifiants Windows"))
                .child(Input::new(&self.api_key))
                .child(div().text_sm().child("Langues par défaut (modifiables dans l’aperçu)"))
                .child(h_flex().gap_2()
                    .child(Select::new(&self.source).title_prefix("Source : ").flex_1())
                    .child(Select::new(&self.target).title_prefix("Cible : ").flex_1()))
                .child(div().text_sm().child("Raccourci global"))
                .child(h_flex().gap_3()
                    .child(div().flex_1().child(self.hotkey.clone()))
                    .child(div().id("hotkey-recorder").track_focus(&self.focus)
                        .on_key_down(cx.listener(|this, event, _, cx| this.record(event, cx)))
                        .child(Button::new("record").label(if self.recording { "Appuie sur le raccourci…" } else { "Changer le raccourci" })
                            .on_click(cx.listener(|this, _, window, cx| { this.recording = true; this.focus.focus(window, cx); this.status = "Appuie sur la combinaison souhaitée (Échap pour annuler).".into(); cx.notify(); })))))
                .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Fermer les fenêtres laisse l’application dans le tray. Pour arrêter : tray → Quitter.")))
            .child(div().text_sm().max_h(px(100.)).id("settings-status").overflow_y_scroll().child(self.status.clone()))
            .child(h_flex().gap_2()
                .child(Button::new("test").label(if self.testing { "Test en cours…" } else { "Tester la connexion" }).disabled(self.testing)
                    .on_click(cx.listener(|this, _, window, cx| this.test(window, cx))))
                .child(div().flex_1())
                .child(Button::new("close").ghost().label("Fermer").on_click(|_, window, _| window.remove_window()))
                .child(Button::new("save").primary().label("Enregistrer").disabled(self.recording)
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx)))))
    }
}
