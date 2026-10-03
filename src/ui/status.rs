use crate::i18n::t;
use crate::{app::Controller, platform::desktop, settings::Operation};
use anyhow::Result;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        button::{Button, ButtonVariants},
        *,
    },
    *,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub struct StatusView {
    pub message: String,
    pub terminal: bool,
    pub error: bool,
    operation: Operation,
    controller: WeakEntity<Controller>,
    layout: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    viewport: Rc<Cell<gpui_kit::Size<Pixels>>>,
    placement: desktop::Placement,
    height: Rc<Cell<f32>>,
    requested_placement: Rc<Cell<desktop::Placement>>,
}

impl StatusView {
    pub fn new(
        operation: Operation,
        controller: WeakEntity<Controller>,
        placement: desktop::Placement,
    ) -> Self {
        Self {
            message: t("Capturing text… Release the shortcut keys.").into(),
            terminal: false,
            error: false,
            operation,
            controller,
            layout: Rc::default(),
            viewport: Rc::default(),
            placement,
            height: Rc::new(Cell::new(placement.height)),
            requested_placement: Rc::new(Cell::new(placement)),
        }
    }

    pub fn show(&self, window: &Window, cx: &App) -> Result<()> {
        Self::schedule_placement(window, self.requested_placement.clone(), cx)
    }

    fn schedule_placement(
        window: &Window,
        placement: Rc<Cell<desktop::Placement>>,
        cx: &App,
    ) -> Result<()> {
        let handle = desktop::native_window(window)?;
        let scale = window.scale_factor();
        // Run outside a Window/App update; native resizing can re-enter GPUI.
        cx.foreground_executor()
            .spawn(async move {
                if let Err(error) =
                    desktop::show_status_without_activation(handle, placement.get(), scale)
                {
                    tracing::error!(%error, "Unable to show the status window");
                }
            })
            .detach();
        Ok(())
    }

    pub(crate) fn smoke_nonactivating(window: &Window, placement: desktop::Placement) -> bool {
        desktop::native_window(window).is_ok_and(|handle| {
            desktop::status_is_nonactivating(handle)
                && desktop::status_has_expected_bounds(handle, placement, window.scale_factor())
        })
    }

    pub(crate) fn smoke_layout(&self) {
        let bounds = self.layout.borrow();
        let message = bounds.get(1).expect("The status message must be measured");
        let footer = bounds.get(2).expect("Error buttons must be visible");
        let size = self.viewport.get();
        assert!(
            message.size.width <= size.width - px(24.),
            "The message must fit the popup width: {message:?}, window: {size:?}"
        );
        assert!(
            message.size.height > px(20.),
            "The long error must wrap: {message:?}"
        );
        assert!(
            footer.bottom() <= size.height,
            "The buttons must remain visible: {footer:?}, window: {size:?}"
        );
        assert!(
            size.height < px(160.),
            "The popup must fit its content: {size:?}"
        );
    }

    pub(crate) fn smoke_placement(&self) -> desktop::Placement {
        desktop::Placement {
            y: self.placement.y + self.placement.height - self.height.get(),
            height: self.height.get(),
            ..self.placement
        }
    }
}

impl Render for StatusView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w(px(self.placement.width))
            .p_3()
            .gap_2()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_children_prepainted({
                let layout = self.layout.clone();
                let viewport = self.viewport.clone();
                let height = self.height.clone();
                let placement = self.placement;
                let requested_placement = self.requested_placement.clone();
                move |bounds, window, cx| {
                    let content_height = bounds
                        .first()
                        .zip(bounds.last())
                        .map(|(first, last)| f32::from(last.bottom() - first.origin.y) + 24.)
                        .unwrap_or(64.)
                        .ceil()
                        .clamp(64., placement.height);
                    *layout.borrow_mut() = bounds;
                    viewport.set(window.viewport_size());
                    if (height.get() - content_height).abs() >= 1. {
                        height.set(content_height);
                        let resized = desktop::Placement {
                            y: placement.y + placement.height - content_height,
                            height: content_height,
                            ..placement
                        };
                        requested_placement.set(resized);
                        let _ = Self::schedule_placement(window, requested_placement.clone(), cx);
                    }
                }
            })
            .child(
                h_flex()
                    .flex_shrink_0()
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
                    .w_full()
                    .min_h(px(0.))
                    .max_h(px(80.))
                    .overflow_y_scroll()
                    .text_sm()
                    .whitespace_normal()
                    .child(crate::i18n::localize_message(&self.message)),
            )
            .when(self.error, |view| {
                view.child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_2()
                        .child(
                            Button::new("status-settings")
                                .ghost()
                                .small()
                                .label(t("Settings"))
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
                                .small()
                                .label(t("Close"))
                                .disabled(!self.terminal)
                                .on_click(|_, window, _| window.remove_window()),
                        ),
                )
            })
    }
}
