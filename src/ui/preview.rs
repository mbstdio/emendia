use crate::i18n::{canonical_language, t};
use crate::{
    app::Controller,
    platform::desktop::{self, Selection},
    settings::{self, CorrectionStyle, Operation, Settings},
    translation::Translator,
    ui::{LanguageSelect, language_select, style_buttons},
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
use std::{cell::RefCell, rc::Rc};
use tokio::{runtime::Handle, task::AbortHandle};

struct PreviewDraft {
    configuration: (String, String, CorrectionStyle),
    text: Entity<TextareaState>,
}

pub struct Preview {
    selection: Selection,
    settings: Settings,
    translator: Translator,
    runtime: Handle,
    controller: WeakEntity<Controller>,
    source: LanguageSelect,
    target: LanguageSelect,
    operation: Operation,
    translation: Entity<TextareaState>,
    drafts: Vec<PreviewDraft>,
    status: String,
    busy: bool,
    replacing: bool,
    copying: bool,
    focus: FocusHandle,
    original_visible: bool,
    request_version: u64,
    pending: Option<AbortHandle>,
    smoke_layout: Option<Rc<RefCell<Vec<Bounds<Pixels>>>>>,
    #[cfg(target_os = "linux")]
    smoke_native_window: Option<isize>,
    _subscriptions: Vec<Subscription>,
}

impl Preview {
    pub fn localize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::ui::refresh_language(&self.source, true, window, cx);
        crate::ui::refresh_language(&self.target, false, window, cx);
        self.translation.update(cx, |state, cx| {
            state.set_placeholder(self.operation.pending(), window, cx)
        });
        window.set_window_title(self.operation.title());
        cx.notify();
    }

    pub(crate) fn enable_smoke_layout(&mut self, window: &Window) {
        self.smoke_layout = Some(Rc::new(RefCell::new(Vec::new())));
        #[cfg(target_os = "linux")]
        {
            self.smoke_native_window = desktop::native_window(window).ok();
        }
        #[cfg(not(target_os = "linux"))]
        let _ = window;
    }

    pub fn set_result(
        &mut self,
        text: String,
        status: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.translation
            .update(cx, |state, cx| state.set_value(text, window, cx));
        self.status = status;
        cx.notify();
    }

    pub(crate) fn smoke_result(&self, cx: &App) -> bool {
        let expected = match self.operation {
            Operation::Translation => "Hello, this is a translation test.",
            Operation::Correction => "Bonjour, ceci est un test de correction.",
        };
        if !self.busy && self.translation.read(cx).value() == expected {
            #[cfg(target_os = "linux")]
            if let Some(window) = self.smoke_native_window {
                assert!(
                    crate::platform::linux::x11::smoke_window_has_icon(window),
                    "The preview must expose the fixed app logo to the X11 taskbar"
                );
                assert!(
                    crate::platform::linux::x11::smoke_preview_is_movable(window),
                    "The preview must be a movable managed window, not a notification"
                );
            }
            if let Some(layout) = &self.smoke_layout {
                let bounds = layout.borrow();
                let Some(header) = bounds.first() else {
                    return false;
                };
                let Some(editor) = bounds.get(2) else {
                    return false;
                };
                assert!(
                    header.size.height <= px(40.),
                    "The selector must remain compact: {header:?}, editor: {editor:?}"
                );
                assert!(
                    editor.size.height >= px(120.),
                    "The editor must fill the available space: {editor:?}"
                );
            }
            true
        } else {
            false
        }
    }

    pub(crate) fn smoke_recovery_result(&self, cx: &App) -> bool {
        self.smoke_result(cx) && self.status.starts_with(self.operation.quick_title())
    }

    pub fn new(
        selection: Selection,
        configuration: (Settings, Operation),
        translator: Translator,
        runtime: Handle,
        controller: WeakEntity<Controller>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (settings, operation) = configuration;
        let source = language_select(&settings.source_language, true, window, cx);
        let target = language_select(&settings.target_language, false, window, cx);
        let translation =
            cx.new(|cx| TextareaState::new(window, cx).placeholder(operation.pending()));
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
                        let language = canonical_language(language);
                        if language == this.settings.source_language {
                            return;
                        }
                        this.settings.source_language = language.to_owned();
                        this.translate(false, window, cx);
                    }
                },
            ),
            cx.subscribe_in(
                &target,
                window,
                |this, _, event: &SelectEvent<Vec<String>>, window, cx| {
                    if let SelectEvent::Confirm(Some(language)) = event {
                        let language = canonical_language(language);
                        if language == this.settings.target_language {
                            return;
                        }
                        this.settings.target_language = language.to_owned();
                        this.translate(false, window, cx);
                    }
                },
            ),
        ];
        let drafts = vec![PreviewDraft {
            configuration: (
                settings.source_language.clone(),
                settings.target_language.clone(),
                settings.correction_style,
            ),
            text: translation.clone(),
        }];
        Self {
            selection,
            settings,
            translator,
            runtime,
            controller,
            source,
            target,
            operation,
            translation,
            drafts,
            status: operation.pending().into(),
            busy: false,
            replacing: false,
            copying: false,
            focus,
            original_visible: false,
            request_version: 0,
            pending: None,
            smoke_layout: None,
            #[cfg(target_os = "linux")]
            smoke_native_window: None,
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
        // Each language/style combination owns its editor and undo history. An
        // old-language result is recoverable by returning to that combination,
        // but is never shown or replaceable as a result for another language.
        if !alternative {
            let configuration = (
                settings.source_language.clone(),
                settings.target_language.clone(),
                settings.correction_style,
            );
            if let Some(draft) = self
                .drafts
                .iter()
                .find(|draft| draft.configuration == configuration)
            {
                self.translation = draft.text.clone();
            } else {
                self.translation = cx
                    .new(|cx| TextareaState::new(window, cx).placeholder(self.operation.pending()));
                self.drafts.push(PreviewDraft {
                    configuration,
                    text: self.translation.clone(),
                });
            }
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
            t("Generating a new suggestion…")
        } else {
            self.operation.pending()
        }
        .into();
        let operation = self.operation;
        let task = self.runtime.spawn(async move {
            translator
                .process(&settings, &key, &original, previous.as_deref(), operation)
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
                            .update(cx, |state, cx| state.replace_all(text, window, cx));
                        this.status = t("Ready — you can edit the result before replacing.").into();
                    }
                    Ok(Err(error)) => this.status = error.to_string(),
                    Err(error) if error.is_cancelled() => {
                        this.status = t("Processing cancelled.").into()
                    }
                    Err(_) => this.status = t("The processing service failed.").into(),
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
        self.status = t("Replacing in the original window…").into();
        let task = self
            .runtime
            .spawn_blocking(move || desktop::replace(&selection, &text));
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
                        this.status = t("Replacement failed. Use Copy.").into();
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
            .spawn_blocking(move || desktop::copy_text(&text));
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.copying = false;
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
            .when_some(self.smoke_layout.clone(), |view, layout| {
                view.on_children_prepainted(move |bounds, _, _| {
                    *layout.borrow_mut() = bounds;
                })
            })
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && !this.replacing {
                    window.remove_window();
                    cx.stop_propagation();
                }
            }))
            .when(self.operation == Operation::Correction, |view| {
                view.child(
                    h_flex().h_8().flex_shrink_0().child(
                        style_buttons("correction-modes", self.settings.correction_style)
                            .disabled(self.replacing || self.copying)
                            .on_click(cx.listener(|this, indices: &Vec<usize>, window, cx| {
                                if let Some(style) = indices
                                    .first()
                                    .and_then(|index| CorrectionStyle::ALL.get(*index))
                                    .copied()
                                    && style != this.settings.correction_style
                                {
                                    this.settings.correction_style = style;
                                    this.translate(false, window, cx);
                                }
                            })),
                    ),
                )
            })
            .when(self.operation == Operation::Translation, |view| {
                view.child(
                    h_flex()
                        .h_8()
                        .flex_shrink_0()
                        .gap_2()
                        .child(
                            Select::new(&self.source)
                                .title_prefix(t("Source: "))
                                .disabled(self.replacing || self.copying)
                                .flex_1(),
                        )
                        .child(
                            Select::new(&self.target)
                                .title_prefix(t("Target: "))
                                .disabled(self.replacing || self.copying)
                                .flex_1(),
                        ),
                )
            })
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        Button::new("original")
                            .ghost()
                            .label(t(if self.original_visible {
                                "Hide original"
                            } else {
                                "Show original"
                            }))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.original_visible = !this.original_visible;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("settings")
                            .ghost()
                            .label(t("Settings"))
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
                    .child(crate::i18n::localize_message(&self.status)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("alternative")
                            .label(t(if has_text { "New suggestion" } else { "Retry" }))
                            .disabled(unavailable)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.translate(has_text, window, cx)
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("cancel")
                            .ghost()
                            .label(t("Cancel"))
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
                            .label(t("Copy"))
                            .disabled(unavailable || !has_text)
                            .on_click(cx.listener(|this, _, window, cx| this.copy(window, cx))),
                    )
                    .child(
                        Button::new("replace")
                            .primary()
                            .label(t("Replace"))
                            .disabled(unavailable || !has_text)
                            .on_click(cx.listener(|this, _, window, cx| this.replace(window, cx))),
                    ),
            )
    }
}
