use crate::{
    app::Controller,
    i18n::{canonical_language, t},
    settings::{self, CorrectionStyle, Operation, Settings},
    translation::Translator,
    ui::{LanguageSelect, language_select, style_buttons},
};
use gpui_kit::{
    assets::IconName as AssetIconName,
    component::{
        button::{Button, ButtonVariants},
        input::{Textarea, TextareaState},
        select::{Select, SelectEvent},
        *,
    },
    prelude::FluentBuilder,
    *,
};
use std::{cell::RefCell, rc::Rc};
use tokio::{runtime::Handle, task::AbortHandle};

/// Session-only drafts. Text never goes into the settings file.
pub struct EditorDraft {
    source: [String; 2],
    result: [String; 2],
    operation: Operation,
    settings: Settings,
}

impl EditorDraft {
    pub fn new(settings: Settings) -> Self {
        Self {
            source: Default::default(),
            result: Default::default(),
            operation: Operation::Translation,
            settings,
        }
    }
}

pub struct Editor {
    source_text: [Entity<TextareaState>; 2],
    result_text: [Entity<TextareaState>; 2],
    source_language: LanguageSelect,
    target_language: LanguageSelect,
    operation: Operation,
    settings: Settings,
    controller: WeakEntity<Controller>,
    translator: Translator,
    runtime: Handle,
    status: String,
    busy: bool,
    pending: Option<AbortHandle>,
    _subscriptions: Vec<Subscription>,
}

impl Editor {
    pub fn new(
        draft: Rc<RefCell<EditorDraft>>,
        controller: WeakEntity<Controller>,
        translator: Translator,
        runtime: Handle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let saved = draft.borrow();
        let settings = saved.settings.clone();
        let source_language = language_select(&settings.source_language, true, window, cx);
        let target_language = language_select(&settings.target_language, false, window, cx);
        let source_text = std::array::from_fn(|index| {
            cx.new(|cx| {
                let mut state =
                    TextareaState::new(window, cx).placeholder(t("Type or paste your text here…"));
                state.set_value(saved.source[index].clone(), window, cx);
                state
            })
        });
        let result_text = std::array::from_fn(|index| {
            cx.new(|cx| {
                let mut state =
                    TextareaState::new(window, cx).placeholder(t("Your result will appear here."));
                state.set_value(saved.result[index].clone(), window, cx);
                state
            })
        });
        let operation = saved.operation;
        drop(saved);
        cx.on_release(move |view, cx| {
            let mut saved = draft.borrow_mut();
            saved.source =
                std::array::from_fn(|i| view.source_text[i].read(cx).value().to_string());
            saved.result =
                std::array::from_fn(|i| view.result_text[i].read(cx).value().to_string());
            saved.operation = view.operation;
            saved.settings = view.settings.clone();
        })
        .detach();
        let mut subscriptions = vec![
            cx.subscribe_in(
                &source_language,
                window,
                |this, _, event: &SelectEvent<Vec<String>>, _, cx| {
                    if let SelectEvent::Confirm(Some(value)) = event {
                        this.settings.source_language = canonical_language(value).to_owned();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &target_language,
                window,
                |this, _, event: &SelectEvent<Vec<String>>, _, cx| {
                    if let SelectEvent::Confirm(Some(value)) = event {
                        this.settings.target_language = canonical_language(value).to_owned();
                        cx.notify();
                    }
                },
            ),
        ];
        for text in source_text.iter().chain(result_text.iter()) {
            subscriptions.push(cx.observe(text, |_, _, cx| cx.notify()));
        }
        Self {
            source_text,
            result_text,
            source_language,
            target_language,
            operation,
            settings,
            controller,
            translator,
            runtime,
            status: String::new(),
            busy: false,
            pending: None,
            _subscriptions: subscriptions,
        }
    }

    fn index(&self) -> usize {
        usize::from(self.operation == Operation::Correction)
    }

    pub fn localize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::ui::refresh_language(&self.source_language, true, window, cx);
        crate::ui::refresh_language(&self.target_language, false, window, cx);
        for text in &self.source_text {
            text.update(cx, |state, cx| {
                state.set_placeholder(t("Type or paste your text here…"), window, cx)
            });
        }
        for text in &self.result_text {
            text.update(cx, |state, cx| {
                state.set_placeholder(t("Your result will appear here."), window, cx)
            });
        }
        cx.notify();
    }

    fn process(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.index();
        let text = self.source_text[index].read(cx).value().to_string();
        if self.busy || text.trim().is_empty() {
            return;
        }
        let Some(controller) = self.controller.upgrade() else {
            return;
        };
        let mut settings = controller.read(cx).settings.clone();
        settings.source_language = self.settings.source_language.clone();
        settings.target_language = self.settings.target_language.clone();
        settings.correction_style = self.settings.correction_style;
        if let Err(error) = settings.validate() {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        let operation = self.operation;
        let translator = self.translator.clone();
        self.busy = true;
        self.status = operation.pending().into();
        let task = self.runtime.spawn(async move {
            let endpoint = settings.base_url.clone();
            let key = tokio::task::spawn_blocking(move || settings::load_api_key(&endpoint))
                .await
                .map_err(|_| anyhow::anyhow!(t("Unable to read the API key")))??;
            translator
                .process(&settings, &key, &text, None, operation)
                .await
        });
        self.pending = Some(task.abort_handle());
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.pending = None;
                match result {
                    Ok(Ok(text)) => {
                        this.result_text[index]
                            .update(cx, |state, cx| state.replace_all(text, window, cx));
                        this.status = t("Ready — you can edit or copy the result.").into();
                    }
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(_) => this.status = t("The processing service failed.").into(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let text = self.result_text[self.index()].read(cx).value().to_string();
        self.busy = true;
        let task = self
            .runtime
            .spawn_blocking(move || crate::platform::desktop::copy_text(&text));
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                this.status = match result {
                    Ok(Ok(())) => t("Result copied to the clipboard.").into(),
                    Ok(Err(error)) => error.to_string(),
                    Err(_) => t("Unable to copy.").into(),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn transfer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let index = self.index();
        let result = self.result_text[index].read(cx).value().to_string();
        let text = if result.trim().is_empty() {
            self.source_text[index].read(cx).value().to_string()
        } else {
            result
        };
        if text.trim().is_empty() {
            return;
        }
        let destination = 1 - index;
        self.source_text[destination].update(cx, |state, cx| state.replace_all(text, window, cx));
        self.result_text[destination]
            .update(cx, |state, cx| state.replace_all(String::new(), window, cx));
        self.operation = if destination == 0 {
            Operation::Translation
        } else {
            Operation::Correction
        };
        self.status = t("Text transferred — choose your options and start processing.").into();
        cx.notify();
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        if let Some(task) = self.pending.take() {
            task.abort();
        }
    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let index = self.index();
        let has_source = !self.source_text[index].read(cx).value().trim().is_empty();
        let has_result = !self.result_text[index].read(cx).value().trim().is_empty();
        v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                h_flex()
                    .gap_2()
                    .children(
                        [Operation::Translation, Operation::Correction]
                            .into_iter()
                            .enumerate()
                            .map(|(i, operation)| {
                                Button::new(("editor-tool", i))
                                    .icon(Icon::new(if i == 0 {
                                        AssetIconName::Languages
                                    } else {
                                        AssetIconName::SpellCheck
                                    }))
                                    .label(t(if i == 0 {
                                        "Translate"
                                    } else {
                                        "Correct / rephrase"
                                    }))
                                    .selected(self.operation == operation)
                                    .disabled(self.busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.operation = operation;
                                        this.status.clear();
                                        cx.notify();
                                    }))
                            }),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("editor-settings")
                            .ghost()
                            .icon(IconName::Settings)
                            .tooltip(t("Settings"))
                            .accessibility_label(t("Settings"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this
                                    .controller
                                    .update(cx, |app, cx| app.open_settings(None, cx));
                            })),
                    ),
            )
            .when(self.operation == Operation::Translation, |view| {
                view.child(
                    h_flex()
                        .gap_2()
                        .child(
                            Select::new(&self.source_language)
                                .title_prefix(t("Source: "))
                                .disabled(self.busy)
                                .flex_1(),
                        )
                        .child(
                            Select::new(&self.target_language)
                                .title_prefix(t("Target: "))
                                .disabled(self.busy)
                                .flex_1(),
                        ),
                )
            })
            .when(self.operation == Operation::Correction, |view| {
                view.child(
                    style_buttons("editor-styles", self.settings.correction_style)
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, indices: &Vec<usize>, _, cx| {
                            if let Some(style) =
                                indices.first().and_then(|i| CorrectionStyle::ALL.get(*i))
                            {
                                this.settings.correction_style = *style;
                                cx.notify();
                            }
                        })),
                )
            })
            .child(div().text_sm().child(t("Your text")))
            .child(
                div().flex_1().min_h(px(100.)).child(
                    Textarea::new(&self.source_text[index])
                        .h_full()
                        .disabled(self.busy),
                ),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("editor-process")
                            .primary()
                            .label(t(if index == 0 {
                                "Translate"
                            } else {
                                "Correct / rephrase"
                            }))
                            .disabled(self.busy || !has_source)
                            .on_click(cx.listener(|this, _, window, cx| this.process(window, cx))),
                    )
                    .child(
                        Button::new("editor-transfer")
                            .icon(Icon::new(if index == 0 {
                                AssetIconName::SpellCheck
                            } else {
                                AssetIconName::Languages
                            }))
                            .label(t(if index == 0 {
                                "Send to proofreading"
                            } else {
                                "Send to translation"
                            }))
                            .tooltip(t(if has_result {
                                "Transfer the result to the other tool"
                            } else {
                                "Transfer your text to the other tool"
                            }))
                            .disabled(self.busy || (!has_source && !has_result))
                            .on_click(cx.listener(|this, _, window, cx| this.transfer(window, cx))),
                    ),
            )
            .child(div().text_sm().child(t("Result")))
            .child(
                div().flex_1().min_h(px(100.)).child(
                    Textarea::new(&self.result_text[index])
                        .h_full()
                        .disabled(self.busy),
                ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(crate::i18n::localize_message(&self.status)),
                    )
                    .child(
                        Button::new("editor-copy")
                            .label(t("Copy"))
                            .disabled(self.busy || !has_result)
                            .on_click(cx.listener(|this, _, window, cx| this.copy(window, cx))),
                    ),
            )
    }
}
