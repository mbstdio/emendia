pub mod preview;
pub mod settings;
pub mod status;
pub mod theme;
use crate::i18n::{canonical_language, t};

use gpui_kit::{
    component::{
        IndexPath, Selectable, Sizable,
        button::{Button, ButtonGroup},
        input::InputState,
        select::SelectState,
    },
    *,
};

pub type LanguageSelect = Entity<SelectState<Vec<String>>>;

pub fn style_buttons(id: &'static str, value: crate::settings::CorrectionStyle) -> ButtonGroup {
    ButtonGroup::new(id).small().compact().children(
        crate::settings::CorrectionStyle::ALL
            .into_iter()
            .enumerate()
            .map(|(index, style)| {
                Button::new((id, index))
                    .child(div().text_xs().child(match style {
                        crate::settings::CorrectionStyle::Faithful => t("Faithful"),
                        crate::settings::CorrectionStyle::Fluent => t("Fluent"),
                        crate::settings::CorrectionStyle::Professional => "Pro",
                        crate::settings::CorrectionStyle::Casual => t("Casual"),
                        crate::settings::CorrectionStyle::Concise => t("Concise"),
                    }))
                    .accessibility_label(style.label())
                    .h(px(28.))
                    .px_2p5()
                    .py_1()
                    .tooltip(style.label())
                    .selected(style == value)
            }),
    )
}

pub fn language_select(
    value: &str,
    automatic: bool,
    window: &mut Window,
    cx: &mut App,
) -> LanguageSelect {
    let (values, index) = language_items(value, automatic);
    cx.new(|cx| SelectState::new(values, index, window, cx).searchable(true))
}

fn language_items(value: &str, automatic: bool) -> (Vec<String>, Option<IndexPath>) {
    let value = canonical_language(value);
    let mut values: Vec<String> = crate::settings::LANGUAGES
        .iter()
        .map(|v| t(v).to_owned())
        .collect();
    if automatic {
        values.insert(0, t(crate::settings::AUTO).into());
    }
    if !values.iter().any(|v| canonical_language(v) == value) {
        values.push(value.into());
    }
    let index = values
        .iter()
        .position(|v| canonical_language(v) == value)
        .map(IndexPath::new);
    (values, index)
}

pub fn refresh_language(
    select: &LanguageSelect,
    automatic: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let value = select
        .read(cx)
        .selected_value()
        .map(|v| canonical_language(v).to_owned())
        .unwrap_or_default();
    let (values, index) = language_items(&value, automatic);
    select.update(cx, |state, cx| {
        state.set_items(values, window, cx);
        state.set_selected_index(index, window, cx);
    });
}

pub fn text_input(value: &str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    cx.new(|cx| {
        let mut state = InputState::new(window, cx);
        state.set_value(value, window, cx);
        state
    })
}
