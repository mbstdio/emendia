use crate::{app::Controller, platform::windows, settings::Operation};
use anyhow::{Result, bail};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        *,
    },
    *,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub struct StatusView {
    pub message: String,
    pub terminal: bool,
    pub error: bool,
    operation: Operation,
    controller: WeakEntity<Controller>,
}

impl StatusView {
    pub fn new(operation: Operation, controller: WeakEntity<Controller>) -> Self {
        Self {
            message: "Capture du texte… Relâche les touches du raccourci.".into(),
            terminal: false,
            error: false,
            operation,
            controller,
        }
    }

    pub fn show(window: &Window, placement: windows::Placement) -> Result<()> {
        let handle = HasWindowHandle::window_handle(window)
            .map_err(|error| anyhow::anyhow!("Fenêtre d’état inaccessible : {error}"))?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            bail!("Fenêtre Windows attendue")
        };
        windows::show_status_without_activation(handle.hwnd.get(), placement)
    }

    pub(crate) fn smoke_nonactivating(window: &Window, placement: windows::Placement) -> bool {
        HasWindowHandle::window_handle(window).is_ok_and(|handle| match handle.as_raw() {
            RawWindowHandle::Win32(handle) => {
                windows::status_is_nonactivating(handle.hwnd.get())
                    && windows::status_has_expected_bounds(handle.hwnd.get(), placement)
            }
            _ => false,
        })
    }
}

impl Render for StatusView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_3()
            .gap_2()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                h_flex()
                    .gap_2()
                    .when(!self.terminal, |view| view.child(Spinner::new().small()))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child(self.operation.title()),
                    ),
            )
            .child(
                div()
                    .id("operation-status")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .text_sm()
                    .child(self.message.clone()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("status-settings")
                            .ghost()
                            .label("Paramètres")
                            .disabled(!self.error)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this.controller.update(cx, |app, cx| {
                                    app.open_settings(Some(this.message.clone()), cx)
                                });
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("status-close")
                            .ghost()
                            .label("Fermer")
                            .disabled(!self.terminal)
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            )
    }
}
