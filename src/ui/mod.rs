pub mod preview;
pub mod settings;

use gpui_kit::{
    component::{IndexPath, input::InputState, select::SelectState},
    *,
};

pub type LanguageSelect = Entity<SelectState<Vec<String>>>;

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
