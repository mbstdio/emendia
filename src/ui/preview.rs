use crate::{
    app::Controller,
    platform::windows::{self, Selection},
    settings::{self, Settings},
    translation::Translator,
    ui::{LanguageSelect, language_select},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        input::{Textarea, TextareaState},
        select::{Select, SelectEvent},
        *,
    },
    *,
};
use tokio::{runtime::Handle, task::AbortHandle};

pub struct Preview {
    selection: Selection,
    settings: Settings,
    translator: Translator,
    runtime: Handle,
    controller: WeakEntity<Controller>,
    source: LanguageSelect,
    target: LanguageSelect,
    translation: Entity<TextareaState>,
    status: String,
    busy: bool,
    replacing: bool,
    copying: bool,
    focus: FocusHandle,
    original_visible: bool,
    request_version: u64,
    pending: Option<AbortHandle>,
    _subscriptions: Vec<Subscription>,
}

impl Preview {
    pub(crate) fn smoke_result(&self, cx: &App) -> bool {
        !self.busy && self.translation.read(cx).value() == "Hello, this is a translation test."
    }

    pub fn new(
        selection: Selection,
        settings: Settings,
        translator: Translator,
        runtime: Handle,
        controller: WeakEntity<Controller>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let source = language_select(&settings.source_language, true, window, cx);
        let target = language_select(&settings.target_language, false, window, cx);
        let translation =
            cx.new(|cx| TextareaState::new(window, cx).placeholder("Traduction en cours…"));
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            weak.upgrade().is_none_or(|view| !view.read(cx).replacing)
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &source,
                window,
                |this, _, event: &SelectEvent<Vec<String>>, window, cx| {
                    if let SelectEvent::Confirm(Some(language)) = event {
                        this.settings.source_language = language.clone();
                        this.translate(false, window, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &target,
                window,
                |this, _, event: &SelectEvent<Vec<String>>, window, cx| {
                    if let SelectEvent::Confirm(Some(language)) = event {
                        this.settings.target_language = language.clone();
                        this.translate(false, window, cx);
                    }
                },
            ),
        ];
        Self {
            selection,
            settings,
            translator,
            runtime,
            controller,
            source,
            target,
            translation,
            status: "Traduction en cours…".into(),
            busy: false,
            replacing: false,
            copying: false,
            focus,
            original_visible: false,
            request_version: 0,
            pending: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn translate(&mut self, alternative: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.replacing || self.copying {
            return;
        }
        // Apply provider/model edits made from this popup's Settings button, while
        // preserving its per-session language selection.
        if let Some(controller) = self.controller.upgrade() {
            let current = &controller.read(cx).settings;
            self.settings.base_url = current.base_url.clone();
            self.settings.model = current.model.clone();
        }
        if let Some(task) = self.pending.take() {
            task.abort();
        }
        self.request_version += 1;
        let version = self.request_version;
        let translator = self.translator.clone();
        let settings = self.settings.clone();
        let original = self.selection.text.clone();
        let previous = alternative.then(|| self.translation.read(cx).value().to_string());
        // A language change invalidates the old result; it must never remain replaceable.
        if !alternative {
            self.translation
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
        let key = match settings::load_api_key(&settings.base_url) {
            Ok(key) => key,
            Err(error) => {
                self.busy = false;
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        self.busy = true;
        self.status = if alternative {
            "Nouvelle proposition en cours…"
        } else {
            "Traduction en cours…"
        }
        .into();
        let task = self.runtime.spawn(async move {
            translator
                .translate(&settings, &key, &original, previous.as_deref())
                .await
        });
        self.pending = Some(task.abort_handle());
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.request_version != version {
                    return;
                }
                this.pending = None;
                this.busy = false;
                match result {
                    Ok(Ok(text)) => {
                        this.translation
                            .update(cx, |state, cx| state.set_value(text, window, cx));
                        this.status =
                            "Prêt — tu peux modifier la traduction avant de remplacer.".into();
                    }
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) if error.is_cancelled() => {
                        this.status = "Traduction annulée.".into()
                    }
                    Err(_) => this.status = "Le service de traduction a échoué.".into(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.replacing || self.copying {
            return;
        }
        let text = self.translation.read(cx).value().to_string();
        let selection = self.selection.clone();
        self.replacing = true;
        self.status = "Remplacement dans la fenêtre d’origine…".into();
        let task = self
            .runtime
            .spawn_blocking(move || windows::replace(&selection, &text));
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.replacing = false;
                match result {
                    Ok(Ok(())) => window.remove_window(),
                    Ok(Err(error)) => {
                        this.status = error.to_string();
                        window.activate_window();
                    }
                    Err(_) => {
                        this.status = "Le remplacement a échoué. Utilise Copier.".into();
                        window.activate_window();
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.translation.read(cx).value().to_string();
        self.copying = true;
        let task = self
            .runtime
            .spawn_blocking(move || windows::copy_text(&text));
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.copying = false;
                this.status = match result {
                    Ok(Ok(())) => "Traduction copiée dans le presse-papiers.".into(),
                    Ok(Err(error)) => error.to_string(),
                    Err(_) => "Copie impossible.".into(),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        if let Some(task) = self.pending.take() {
            task.abort();
        }
    }
}

impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_text = !self.translation.read(cx).value().trim().is_empty();
        let unavailable = self.busy || self.replacing || self.copying;
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .p_4()
            .gap_3()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && !this.replacing {
                    window.remove_window();
                    cx.stop_propagation();
                }
            }))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Select::new(&self.source)
                            .title_prefix("Source : ")
                            .disabled(self.replacing || self.copying)
                            .flex_1(),
                    )
                    .child(
                        Select::new(&self.target)
                            .title_prefix("Cible : ")
                            .disabled(self.replacing || self.copying)
                            .flex_1(),
                    ),
            )
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        Button::new("original")
                            .ghost()
                            .label(if self.original_visible {
                                "Masquer l’original"
                            } else {
                                "Voir l’original"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.original_visible = !this.original_visible;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("settings")
                            .ghost()
                            .label("Paramètres")
                            .disabled(self.replacing)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this
                                    .controller
                                    .update(cx, |app, cx| app.open_settings(None, cx));
                            })),
                    ),
            )
            .when(self.original_visible, |view| {
                view.child(
                    div()
                        .id("original-text")
                        .max_h(px(90.))
                        .overflow_y_scroll()
                        .text_sm()
                        .child(self.selection.text.clone()),
                )
            })
            .child(
                div().flex_1().min_h(px(80.)).child(
                    Textarea::new(&self.translation)
                        .h_full()
                        .disabled(unavailable),
                ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.status.clone()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("alternative")
                            .label(if has_text {
                                "Nouvelle proposition"
                            } else {
                                "Réessayer"
                            })
                            .disabled(unavailable)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.translate(has_text, window, cx)
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("cancel")
                            .ghost()
                            .label("Annuler")
                            .disabled(self.replacing)
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("copy")
                            .label("Copier")
                            .disabled(unavailable || !has_text)
                            .on_click(cx.listener(|this, _, window, cx| this.copy(window, cx))),
                    )
                    .child(
                        Button::new("replace")
                            .primary()
                            .label("Remplacer")
                            .disabled(unavailable || !has_text)
                            .on_click(cx.listener(|this, _, window, cx| this.replace(window, cx))),
                    ),
            )
    }
}
