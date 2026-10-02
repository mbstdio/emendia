pub mod preview;
pub mod settings;
pub mod status;
pub mod theme;

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
                        crate::settings::CorrectionStyle::Faithful => "Fidèle",
                        crate::settings::CorrectionStyle::Fluent => "Fluide",
                        crate::settings::CorrectionStyle::Professional => "Pro",
                        crate::settings::CorrectionStyle::Casual => "Décontracté",
                        crate::settings::CorrectionStyle::Concise => "Concis",
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
    let mut values: Vec<String> = crate::settings::LANGUAGES
        .iter()
        .map(|v| (*v).to_owned())
        .collect();
    if automatic {
        values.insert(0, crate::settings::AUTO.into());
    }
    if !values.iter().any(|v| v == value) {
        values.push(value.into());
    }
    let index = values.iter().position(|v| v == value).map(IndexPath::new);
    cx.new(|cx| SelectState::new(values, index, window, cx).searchable(true))
}

pub fn text_input(value: &str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    cx.new(|cx| {
        let mut state = InputState::new(window, cx);
        state.set_value(value, window, cx);
        state
    })
}
