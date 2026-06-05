pub mod window;

use crate::app::window::WleaveWindow;
use crate::button::{WButton, WButtonActionList};
use crate::config::AppConfig;
use crate::exec::run_command;
use crate::layout::MenuLayout;
use crate::paintable::svg_picture_colorized;
use glib::object::Cast;
use glib::timeout_add_local_once;
use glib_macros::{clone, closure};
use gtk4::prelude::{BoxExt, ButtonExt, GObjectPropertyExpressionExt, GtkWindowExt, WidgetExt};
use gtk4::{EventControllerKey, EventControllerMotion, GestureClick, PropagationPhase};
use gtk4_layer_shell::{KeyboardMode, LayerShell};
use libadwaita::prelude::AdwApplicationWindowExt;
use std::sync::Arc;
use std::time::Duration;
use wleave::options::{ButtonLayout, ButtonState, Protocol};
use wleave::units::{AspectRatio, LengthArgs, LengthDimension};

fn do_exit(window: &WleaveWindow, _service_mode: bool) {
    window.close();
}

fn on_option(
    command_list: &WButtonActionList,
    delay_ms: u32,
    service_mode: bool,
    window: WleaveWindow,
) {
    let Some(command) = command_list.enumerate().find(|w| w.is_applicable()) else {
        return;
    };

    let command = command.clone();

    window.connect_hide(clone!(
        #[strong]
        command,
        move |window| {
            timeout_add_local_once(
                Duration::from_millis(delay_ms.into()),
                clone!(
                    #[strong]
                    command,
                    #[weak_allow_none]
                    window,
                    move || {
                        run_command(command);
                        window.inspect(move |w| do_exit(w, service_mode));
                    }
                ),
            );
        }
    ));

    window.set_visible(false);
}

fn handle_key(
    config: &Arc<AppConfig>,
    window: &WleaveWindow,
    key: &gtk4::gdk::Key,
) -> glib::Propagation {
    if let &gtk4::gdk::Key::Escape = key {
        do_exit(window, config.service);
        return glib::Propagation::Proceed;
    }

    let key = key
        .to_unicode()
        .map(|c| c.to_string())
        .or_else(|| key.name().map(|s| s.to_string()));

    if let Some(ref key_name) = key {
        let button = config.buttons.iter().find(|b| b.keybind == *key_name);

        if let Some(WButton { action, .. }) = button {
            on_option(
                action,
                config.delay_command_ms,
                config.service,
                window.clone(),
            );
        }
    }

    glib::Propagation::Proceed
}

fn apply_button_state(
    button: &gtk4::Button,
    label: &gtk4::Label,
    state: &ButtonState,
    viewport: (f32, f32),
) {
    label.set_label(&state.label);
    label.set_justify(state.justify.into());

    if let Some(width) = state.width {
        label.set_xalign(width);
    }
    if let Some(height) = state.height {
        label.set_yalign(height);
    }

    button.set_margin_start(
        state
            .margins
            .left
            .to_i32(viewport, LengthDimension::Horizontal),
    );
    button.set_margin_end(
        state
            .margins
            .right
            .to_i32(viewport, LengthDimension::Horizontal),
    );
    button.set_margin_top(
        state
            .margins
            .top
            .to_i32(viewport, LengthDimension::Vertical),
    );
    button.set_margin_bottom(
        state
            .margins
            .bottom
            .to_i32(viewport, LengthDimension::Vertical),
    );
}

pub fn create_app(config: &Arc<AppConfig>, app: &libadwaita::Application) -> WleaveWindow {
    let service_mode = config.service;

    let container_box = gtk4::CenterBox::builder()
        .valign(gtk4::Align::Fill)
        .halign(gtk4::Align::Fill)
        .orientation(gtk4::Orientation::Vertical)
        .build();

    let window = WleaveWindow::new(app);
    window.set_content(Some(&container_box));

    window.connect_window_width_notify(clone!(
        #[weak_allow_none]
        container_box,
        #[strong]
        config,
        move |w| {
            let Some(container_box) = container_box else {
                return;
            };

            let arg = LengthArgs {
                viewport: (w.width() as f32, w.height() as f32),
                dimension: LengthDimension::Horizontal,
            };

            container_box.set_margin_start(config.margins.left.0.for_args(&arg) as i32);
            container_box.set_margin_end(config.margins.left.0.for_args(&arg) as i32);
        }
    ));

    window.connect_window_height_notify(clone!(
        #[weak_allow_none]
        container_box,
        #[strong]
        config,
        move |w| {
            let Some(container_box) = container_box else {
                return;
            };

            let arg = LengthArgs {
                viewport: (w.width() as f32, w.height() as f32),
                dimension: LengthDimension::Vertical,
            };

            container_box.set_margin_top(config.margins.top.0.for_args(&arg) as i32);
            container_box.set_margin_bottom(config.margins.bottom.0.for_args(&arg) as i32);
        }
    ));

    match config.protocol {
        Protocol::LayerShell => {
            window.init_layer_shell();
            window.set_layer(gtk4_layer_shell::Layer::Overlay);
            window.set_namespace(Some("wleave"));
            window.set_exclusive_zone(-1);
            window.set_keyboard_mode(KeyboardMode::Exclusive);

            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Right, true);
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
        }
        Protocol::Xdg => {
            window.fullscreen();
        }
        Protocol::None => {}
    }

    if config.close_on_lost_focus {
        window.connect_is_active_notify(move |window| {
            if window.is_visible() && !window.is_active() && !service_mode {
                do_exit(window, service_mode);
            }
        });
    }

    let click_away_controller = GestureClick::builder()
        .propagation_phase(PropagationPhase::Bubble)
        .button(gtk4::gdk::BUTTON_PRIMARY)
        .n_points(1)
        .build();
    click_away_controller.connect_released(clone!(
        #[weak]
        window,
        #[upgrade_or_panic]
        move |_, _, _, _| {
            do_exit(&window, service_mode);
        }
    ));
    window.add_controller(click_away_controller);

    let key_controller = EventControllerKey::new();
    key_controller.connect_key_pressed(clone!(
        #[strong]
        config,
        #[weak]
        window,
        #[upgrade_or_panic]
        move |_, key, _, _| handle_key(&config, &window, &key)
    ));
    window.add_controller(key_controller);

    let btn_count = config.buttons.len() as u32;
    let buttons_per_row = match config.buttons_per_row {
        ButtonLayout::Auto => None,
        ButtonLayout::PerRow(n) => Some(n),
        ButtonLayout::RowRatio(n, d) => Some(btn_count * n / d.min(btn_count * n)),
    };

    let column_spacing = window
        .property_expression_weak("window-width")
        .chain_closure::<f32>(closure!(
            #[strong]
            config,
            move |w: Option<WleaveWindow>, width: i32| {
                config.column_spacing.for_args(&LengthArgs {
                    viewport: (
                        width as f32,
                        w.as_ref()
                            .map(WleaveWindow::window_width)
                            .unwrap_or_default() as f32,
                    ),
                    dimension: LengthDimension::Horizontal,
                })
            }
        ))
        .upcast();

    let row_spacing = window
        .property_expression_weak("window-height")
        .chain_closure::<f32>(closure!(
            #[strong]
            config,
            move |w: Option<WleaveWindow>, height: i32| {
                config.column_spacing.for_args(&LengthArgs {
                    viewport: (
                        w.as_ref()
                            .map(WleaveWindow::window_width)
                            .unwrap_or_default() as f32,
                        height as f32,
                    ),
                    dimension: LengthDimension::Horizontal,
                })
            }
        ))
        .upcast();

    let buttons_container = gtk4::Box::builder()
        .valign(gtk4::Align::Fill)
        .halign(gtk4::Align::Fill)
        .layout_manager(&MenuLayout::new(
            config.button_layout,
            config.button_aspect_ratio.map(AspectRatio::as_float),
            column_spacing,
            row_spacing,
            buttons_per_row,
        ))
        .build();

    for btn in config.buttons.iter() {
        let justify = btn.states.default.justify.into();

        let button = gtk4::Button::builder()
            .name(&btn.states.default.label)
            .hexpand(true)
            .vexpand(true)
            .cursor(&gdk4::Cursor::from_name("pointer", None).expect("pointer cursor not found"))
            .build();

        let overlay = gtk4::Overlay::builder().vexpand(true).hexpand(true).build();

        if config.show_keybinds {
            let key_label = gtk4::Label::builder()
                .label(format!("[{}]", btn.keybind))
                .halign(gtk4::Align::Start)
                .valign(gtk4::Align::Start)
                .css_classes(["dimmed", "keybind"])
                .build();

            overlay.add_overlay(&key_label);
        }

        let inner = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .valign(gtk4::Align::Center)
            .build();

        let icon = if let Some(icn) = &btn.icon {
            let icon = if icn.ends_with(".svg") {
                svg_picture_colorized(icn).upcast()
            } else {
                gtk4::Picture::for_filename(icn)
            };

            icon.set_content_fit(gtk4::ContentFit::ScaleDown);
            icon.add_css_class("icon");

            inner.append(&icon);
            Some(icon)
        } else {
            None
        };

        let label = gtk4::Label::builder()
            .label(&btn.states.default.label)
            .css_classes(["action-name"])
            .use_markup(true)
            .justify(justify)
            .build();

        // Picture being none means the old system to configure buttons is used
        if btn.states.default.width.is_some()
            || btn.states.default.height.is_some()
            || icon.is_none()
        {
            label.set_xalign(btn.states.default.width.unwrap_or(0.5));
            label.set_yalign(btn.states.default.height.unwrap_or(0.9));
            overlay.add_overlay(&label);
        } else {
            inner.insert_child_after(&label, icon.as_ref());
        }

        overlay.set_child(Some(&inner));
        button.add_css_class("button");

        if btn.circular {
            button.add_css_class("circular-button");
        }

        button.connect_clicked(clone!(
            #[weak]
            window,
            #[to_owned(rename_to = action)]
            &btn.action,
            #[to_owned(rename_to = delay_ms)]
            &config.delay_command_ms,
            #[upgrade_or_panic]
            move |_| on_option(&action, delay_ms, service_mode, window)
        ));

        button.set_child(Some(&overlay));

        let btn_states = Arc::new(btn.states.clone());
        let motion_controller = EventControllerMotion::new();
        motion_controller.connect_enter(clone!(
            #[weak]
            btn_states,
            #[weak]
            label,
            #[weak]
            button,
            #[weak]
            window,
            move |_, _x, _y| {
                let viewport = (window.width() as f32, window.height() as f32);
                let state = btn_states.hover.as_ref().unwrap_or(&btn_states.default);
                apply_button_state(&button, &label, state, viewport);
            }
        ));

        motion_controller.connect_leave(clone!(
            #[weak]
            btn_states,
            #[weak]
            label,
            #[weak]
            button,
            #[weak]
            window,
            move |_| {
                let viewport = (window.width() as f32, window.height() as f32);
                apply_button_state(&button, &label, &btn_states.default, viewport);
            }
        ));

        button.add_controller(motion_controller);

        button.connect_state_flags_changed(clone!(
            #[weak]
            btn_states,
            #[weak]
            label,
            #[strong]
            button,
            #[weak]
            window,
            move |_, _flags| {
                let viewport = (window.width() as f32, window.height() as f32);
                let flags = button.state_flags();

                let state = if flags.contains(gtk4::StateFlags::ACTIVE) {
                    btn_states
                        .active
                        .as_ref()
                        .or(btn_states.focus.as_ref())
                        .or(btn_states.hover.as_ref())
                        .unwrap_or(&btn_states.default)
                } else if flags.contains(gtk4::StateFlags::FOCUS_WITHIN) {
                    btn_states
                        .focus
                        .as_ref()
                        .or(btn_states.hover.as_ref())
                        .unwrap_or(&btn_states.default)
                } else if flags.contains(gtk4::StateFlags::PRELIGHT) {
                    btn_states.hover.as_ref().unwrap_or(&btn_states.default)
                } else {
                    &btn_states.default
                };

                apply_button_state(&button, &label, state, viewport);
            }
        ));

        buttons_container.append(&button);
    }

    container_box.set_shrink_center_last(false);
    container_box.set_center_widget(Some(&buttons_container));

    if !config.no_version_info {
        let version_info = gtk4::Label::builder()
        .label(format!(
            "Wleave {}. <a href=\"https://github.com/AMNatty/wleave/releases/tag/0.6.0\">Missing or broken icons?</a>",
            env!("CARGO_PKG_VERSION")
        ))
        .use_markup(true)
        .can_focus(false)
        .css_classes(["dimmed", "version-info"])
        .margin_top(12)
        .build();
        container_box.set_end_widget(Some(&version_info));
    }

    window
}
