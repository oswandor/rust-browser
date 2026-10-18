//! Dibujo del "chrome" del navegador con egui: barra de pestañas + toolbar.
//!
//! Estilo inspirado en Edge/Chrome en tema oscuro:
//! - Franja de pestañas más oscura; la pestaña activa se funde con la toolbar
//!   (mismo color, esquinas superiores redondeadas).
//! - Iconos Material Symbols (sub-set en `assets/icons.ttf`).
//! - Barra de direcciones en forma de píldora con candado en https.
//!
//! No hay CentralPanel: el área restante la ocupa físicamente el webview
//! hijo (ventana nativa encima de la superficie wgpu).

use egui::{Button, Color32, CornerRadius, FontId, Id, Key, Margin, Panel, RichText, TextEdit};

use crate::app::{Action, BrowserState};
use crate::navigation::Tab;

const ADDRESS_ID: &str = "address_bar";

/// Glifos del sub-set de Material Symbols Outlined (ver assets/).
pub mod icon {
    pub const ARROW_BACK: char = '\u{e5c4}';
    pub const ARROW_FORWARD: char = '\u{e5c8}';
    pub const REFRESH: char = '\u{e5d5}';
    pub const HOME: char = '\u{e9b2}';
    pub const CLOSE: char = '\u{e5cd}';
    pub const ADD: char = '\u{e145}';
    pub const LOCK: char = '\u{e899}';
}

/// Paleta (valores aproximados del tema oscuro de Edge/Chrome).
mod colors {
    use super::Color32;
    /// Franja de la barra de pestañas.
    pub const STRIP_BG: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x1f);
    /// Toolbar y pestaña activa.
    pub const CHROME_BG: Color32 = Color32::from_rgb(0x2b, 0x2b, 0x2b);
    /// Barra de direcciones (píldora).
    pub const OMNIBOX_BG: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x1f);
    pub const ICON: Color32 = Color32::from_rgb(0xbd, 0xc1, 0xc6);
    pub const ICON_DISABLED: Color32 = Color32::from_rgb(0x5f, 0x63, 0x68);
    pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xea, 0xed);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xa6);
}

fn icon_font() -> egui::FontFamily {
    egui::FontFamily::Name("material".into())
}

fn icon(codepoint: char, size: f32, color: Color32) -> RichText {
    RichText::new(codepoint)
        .font(FontId::new(size, icon_font()))
        .color(color)
}

/// Botón cuadrado con icono; el hover lo pone el tema de egui (chip claro).
/// Devuelve `(clicked, enabled)`.
fn icon_button(
    ui: &mut egui::Ui,
    codepoint: char,
    enabled: bool,
    tooltip: &str,
) -> (bool, bool) {
    let color = if enabled {
        colors::ICON
    } else {
        colors::ICON_DISABLED
    };
    let response = ui
        .add_sized(
            [34.0, 30.0],
            Button::new(icon(codepoint, 19.0, color)).frame(false),
        )
        .on_hover_text(tooltip);
    (response.clicked(), enabled)
}

/// Pinta la UI superior y acumula las acciones resultantes.
pub fn draw_chrome(ui: &mut egui::Ui, state: &mut BrowserState, actions: &mut Vec<Action>) {
    let ctx = ui.ctx().clone();
    ctx.set_visuals(egui::Visuals::dark());

    // Rect lógico completo de la ventana, medido antes de que los paneles consuman espacio.
    let full = ui.max_rect();

    Panel::top("tab_bar")
        .exact_size(32.0)
        .frame(
            egui::Frame::new()
                .fill(colors::STRIP_BG)
                .inner_margin(Margin::symmetric(6, 0)),
        )
        .show(ui, |ui| draw_tab_bar(ui, state, actions));

    Panel::top("toolbar")
        .exact_size(48.0)
        .frame(
            egui::Frame::new()
                .fill(colors::CHROME_BG)
                .inner_margin(Margin::symmetric(8, 7)),
        )
        .show(ui, |ui| draw_toolbar(ui, state, actions));

    // El rect lógico que queda libre es donde debe vivir el webview.
    let content = ui.available_rect_before_wrap();
    let ppp = ctx.pixels_per_point();
    state.desired_webview_bounds = Some(crate::webview::bounds_from_content(
        content,
        ppp,
        winit_size(full.size(), ppp),
    ));

    // Atajos globales cuando el foco está en la UI nativa (no en la página).
    let (ctrl, alt) = ctx.input(|i| (i.modifiers.ctrl, i.modifiers.alt));
    let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));
    if ctrl && pressed(Key::T) {
        actions.push(Action::NewTab);
    }
    if ctrl && pressed(Key::W) {
        actions.push(Action::CloseTab(state.active));
    }
    if ctrl && pressed(Key::L) {
        actions.push(Action::FocusAddress);
    }
    if pressed(Key::F5) || (ctrl && pressed(Key::R)) {
        actions.push(Action::Reload);
    }
    if alt && pressed(Key::ArrowLeft) {
        actions.push(Action::Back);
    }
    if alt && pressed(Key::ArrowRight) {
        actions.push(Action::Forward);
    }
}

fn winit_size(logical: egui::Vec2, ppp: f32) -> winit::dpi::PhysicalSize<u32> {
    winit::dpi::PhysicalSize::new(
        (logical.x * ppp).round() as u32,
        (logical.y * ppp).round() as u32,
    )
}

fn draw_tab_bar(ui: &mut egui::Ui, state: &mut BrowserState, actions: &mut Vec<Action>) {
    ui.horizontal_centered(|ui| {
        // Dejar sitio al botón "+" tras el área con scroll de pestañas.
        let scroll_width = (ui.available_width() - 38.0).max(60.0);
        egui::ScrollArea::horizontal()
            .id_salt("tabs_scroll")
            .max_width(scroll_width)
            .show(ui, |ui| {
                ui.set_min_height(ui.available_height());
                // Recorremos por índice para poder cerrar sin peleas con el borrow checker.
                let mut close_request: Option<usize> = None;
                for i in 0..state.tabs.len() {
                    let is_active = i == state.active;
                    draw_tab(ui, &state.tabs[i], is_active, i, actions, &mut close_request);
                }
                if let Some(i) = close_request {
                    actions.push(Action::CloseTab(i));
                }
            });

        ui.add_space(2.0);
        ui.add_sized(
            [26.0, 26.0],
            Button::new(icon(icon::ADD, 17.0, colors::ICON)).frame(false),
        )
        .on_hover_text("Nueva pestaña (Ctrl+T)")
        .clicked()
        .then(|| actions.push(Action::NewTab));
    });
}

fn draw_tab(
    ui: &mut egui::Ui,
    tab: &Tab,
    is_active: bool,
    index: usize,
    actions: &mut Vec<Action>,
    close_request: &mut Option<usize>,
) {
    let title: String = if tab.url() == crate::navigation::NEW_TAB_URL {
        "Nueva pestaña".to_string()
    } else if tab.title.is_empty() {
        "Cargando…".to_string()
    } else {
        tab.title.clone()
    };

    // La pestaña activa comparte color con la toolbar y llega al borde inferior
    // de la franja: efecto "pestaña fundida" estilo Edge/Chrome.
    let fill = if is_active {
        colors::CHROME_BG
    } else {
        Color32::TRANSPARENT
    };
    let (text_color, close_color) = if is_active {
        (colors::TEXT, colors::TEXT_DIM)
    } else {
        (colors::TEXT_DIM, colors::TEXT_DIM)
    };

    let frame = egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius {
            nw: 9,
            ne: 9,
            sw: 0,
            se: 0,
        })
        .inner_margin(Margin::symmetric(4, 0))
        .outer_margin(Margin {
            left: 1,
            right: 1,
            top: 2,
            bottom: 0,
        });

    frame.show(ui, |ui| {
        ui.set_min_height(ui.available_height());
        ui.add_space(1.0);
        ui.horizontal_centered(|ui| {
            if tab.loading {
                ui.add(egui::Spinner::new().size(13.0).color(colors::ICON));
                ui.add_space(6.0);
            }

            // Ancho de la pestaña según el texto REAL (estilo Chrome): crece
            // hasta 190 px y a partir de ahí se trunca con elipsis. Nunca usa
            // el ancho disponible de la franja completa.
            let title_w = ui
                .painter()
                .layout_no_wrap(
                    title.clone(),
                    egui::FontId::proportional(14.0),
                    text_color,
                )
                .size()
                .x
                .clamp(90.0, 190.0);

            let label = RichText::new(title).color(text_color).size(14.0);
            let title_resp = ui.add_sized(
                [title_w, ui.available_height()],
                Button::new(label).truncate().frame(false),
            );
            if title_resp.clicked() {
                actions.push(Action::SwitchTab(index));
            }

            let close_resp = ui.add_sized(
                [22.0, 22.0],
                Button::new(icon(icon::CLOSE, 14.0, close_color)).frame(false),
            )
            .on_hover_text("Cerrar pestaña (Ctrl+W)");
            if close_resp.clicked() {
                *close_request = Some(index);
            }
        });
        ui.add_space(1.0);
    });
}

fn draw_toolbar(ui: &mut egui::Ui, state: &mut BrowserState, actions: &mut Vec<Action>) {
    ui.horizontal_centered(|ui| {
        let active_tab = &state.tabs[state.active];

        let (back_clicked, back_on) = icon_button(
            ui,
            icon::ARROW_BACK,
            active_tab.can_back(),
            "Atrás (Alt+←)",
        );
        if back_clicked && back_on {
            actions.push(Action::Back);
        }

        let (fwd_clicked, fwd_on) = icon_button(
            ui,
            icon::ARROW_FORWARD,
            active_tab.can_forward(),
            "Adelante (Alt+→)",
        );
        if fwd_clicked && fwd_on {
            actions.push(Action::Forward);
        }

        let (reload_clicked, _) = icon_button(ui, icon::REFRESH, true, "Recargar (F5 / Ctrl+R)");
        if reload_clicked {
            actions.push(Action::Reload);
        }

        let (home_clicked, _) = icon_button(ui, icon::HOME, true, "Inicio");
        if home_clicked {
            actions.push(Action::Home);
        }

        ui.add_space(6.0);

        // Omnibox en píldora, ocupando el resto de la toolbar.
        let pill = egui::Frame::new()
            .fill(colors::OMNIBOX_BG)
            .corner_radius(16.0)
            .inner_margin(Margin::symmetric(12, 5));
        pill.show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let is_https = state.address.starts_with("https://");
                if is_https {
                    ui.label(icon(icon::LOCK, 14.0, colors::ICON_DISABLED));
                    ui.add_space(7.0);
                }

                let edit = TextEdit::singleline(&mut state.address)
                    .id(Id::new(ADDRESS_ID))
                    .hint_text("Busca o escribe una URL…")
                    .desired_width(f32::INFINITY)
                    .frame(egui::Frame::NONE);
                let response = ui.add(edit);

                // Foco programático (pestaña nueva, Ctrl+L desde la página o la UI).
                if state.focus_address {
                    ui.ctx()
                        .memory_mut(|m| m.request_focus(Id::new(ADDRESS_ID)));
                    state.focus_address = false;
                }

                // Enter navega; Escape devuelve el foco a la página.
                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    if let Some(url) = crate::navigation::normalize_input(&state.address) {
                        actions.push(Action::Navigate(url));
                    }
                }
                if response.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                    ui.ctx()
                        .memory_mut(|m| m.surrender_focus(Id::new(ADDRESS_ID)));
                    actions.push(Action::FocusPage);
                }
            });
        });
    });
}
