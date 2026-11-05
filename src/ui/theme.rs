//! Thème sombre, widgets personnalisés et icônes vectorielles.

use crate::analyze::Severity;
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Margin, Pos2, Rect,
    Response, RichText, Sense, Shape, Stroke, StrokeKind, TextStyle, Ui, pos2, vec2,
};

pub const BG: Color32 = Color32::from_rgb(14, 17, 23);
pub const SIDEBAR: Color32 = Color32::from_rgb(11, 14, 19);
pub const CARD: Color32 = Color32::from_rgb(23, 27, 36);
pub const CARD_HOVER: Color32 = Color32::from_rgb(29, 35, 48);
pub const BORDER: Color32 = Color32::from_rgb(38, 45, 59);
pub const INPUT: Color32 = Color32::from_rgb(16, 20, 27);
pub const TEXT: Color32 = Color32::from_rgb(230, 233, 239);
pub const MUTED: Color32 = Color32::from_rgb(139, 147, 167);
pub const ACCENT: Color32 = Color32::from_rgb(91, 140, 255);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(58, 100, 214);
pub const SUCCESS: Color32 = Color32::from_rgb(52, 211, 153);
pub const WARN: Color32 = Color32::from_rgb(251, 191, 36);
pub const DANGER: Color32 = Color32::from_rgb(248, 113, 113);

pub fn severity_color(s: Severity) -> Color32 {
    match s {
        Severity::Info => ACCENT,
        Severity::Warning => WARN,
        Severity::Blocker => DANGER,
    }
}

pub fn setup(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.extreme_bg_color = INPUT;
    v.faint_bg_color = CARD;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    let r = CornerRadius::same(9);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.bg_fill = Color32::from_rgb(30, 36, 48);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(30, 36, 48);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.hovered.bg_fill = Color32::from_rgb(38, 46, 62);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(38, 46, 62);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    v.widgets.active.bg_fill = ACCENT_DARK;
    v.widgets.active.weak_bg_fill = ACCENT_DARK;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    ctx.set_visuals_of(egui::Theme::Dark, v.clone());
    ctx.set_visuals_of(egui::Theme::Light, v);
    ctx.set_theme(egui::ThemePreference::Dark);

    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(10.0, 10.0);
        s.spacing.button_padding = vec2(16.0, 9.0);
        s.spacing.interact_size.y = 36.0;
        s.spacing.text_edit_width = 260.0;
        s.text_styles.insert(
            TextStyle::Heading,
            FontId::new(28.0, FontFamily::Proportional),
        );
        s.text_styles
            .insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
        s.text_styles.insert(
            TextStyle::Button,
            FontId::new(15.0, FontFamily::Proportional),
        );
        s.text_styles.insert(
            TextStyle::Small,
            FontId::new(12.5, FontFamily::Proportional),
        );
        s.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(12.5, FontFamily::Monospace),
        );
    });
}

pub fn muted(s: impl Into<String>) -> RichText {
    RichText::new(s.into()).color(MUTED)
}

pub fn card<R>(ui: &mut Ui, title: Option<&str>, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(t) = title {
                ui.label(RichText::new(t).strong().size(16.5));
                ui.add_space(4.0);
            }
            add(ui)
        })
        .inner
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BtnKind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

pub fn button(ui: &mut Ui, text: &str, kind: BtnKind) -> Response {
    let white = Color32::WHITE;
    let (fill, hover_fill, press_fill, stroke, hover_stroke, color) = match kind {
        BtnKind::Primary => (
            ACCENT_DARK,
            lerp_color(ACCENT_DARK, white, 0.16),
            lerp_color(ACCENT_DARK, Color32::BLACK, 0.25),
            Stroke::new(1.0, ACCENT),
            Stroke::new(1.5, lerp_color(ACCENT, white, 0.4)),
            Color32::WHITE,
        ),
        BtnKind::Secondary => {
            let f = Color32::from_rgb(30, 36, 48);
            (
                f,
                lerp_color(f, white, 0.10),
                lerp_color(f, Color32::BLACK, 0.3),
                Stroke::new(1.0, BORDER),
                Stroke::new(1.5, ACCENT.gamma_multiply(0.8)),
                TEXT,
            )
        }
        BtnKind::Ghost => (
            Color32::TRANSPARENT,
            Color32::from_white_alpha(18),
            Color32::from_white_alpha(36),
            Stroke::NONE,
            Stroke::new(1.0, BORDER),
            MUTED,
        ),
        BtnKind::Danger => {
            let f = Color32::from_rgb(70, 28, 32);
            (
                f,
                lerp_color(f, DANGER, 0.22),
                lerp_color(f, Color32::BLACK, 0.3),
                Stroke::new(1.0, DANGER.gamma_multiply(0.6)),
                Stroke::new(1.5, DANGER),
                DANGER,
            )
        }
    };
    let resp = ui
        .scope(|ui| {
            let w = &mut ui.style_mut().visuals.widgets;
            let r = CornerRadius::same(10);
            for (st, f, sk) in [
                (&mut w.inactive, fill, stroke),
                (&mut w.hovered, hover_fill, hover_stroke),
                (&mut w.active, press_fill, hover_stroke),
            ] {
                st.weak_bg_fill = f;
                st.bg_fill = f;
                st.bg_stroke = sk;
                st.corner_radius = r;
                st.expansion = 0.0;
            }
            ui.add(
                egui::Button::new(RichText::new(text).color(color).strong())
                    .min_size(vec2(0.0, 38.0)),
            )
        })
        .inner;
    // Éclair bref après un clic : l'utilisateur voit que l'action a été prise en compte.
    let ctx = ui.ctx().clone();
    let flash_id = resp.id.with("flash");
    let now = ctx.input(|i| i.time);
    if resp.clicked() {
        ctx.data_mut(|d| d.insert_temp(flash_id, now));
    }
    if let Some(t0) = ctx.data(|d| d.get_temp::<f64>(flash_id)) {
        let el = (now - t0) as f32;
        if el < 0.45 {
            let a = ((1.0 - el / 0.45) * 80.0) as u8;
            ui.painter().rect_filled(
                resp.rect,
                CornerRadius::same(10),
                Color32::from_white_alpha(a),
            );
            ctx.request_repaint();
        }
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Demande l'affichage d'un message de confirmation (bandeau en bas de la fenêtre).
pub fn toast(ctx: &egui::Context, msg: impl Into<String>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL.with("fm_toast"), msg.into()));
    ctx.request_repaint();
}

pub fn take_toast(ctx: &egui::Context) -> Option<String> {
    ctx.data_mut(|d| {
        let id = egui::Id::NULL.with("fm_toast");
        let v = d.get_temp::<String>(id);
        if v.is_some() {
            d.remove::<String>(id);
        }
        v
    })
}

pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    Frame::new()
        .fill(color.gamma_multiply(0.16))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(20))
        .inner_margin(Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).size(12.5).strong());
        });
}

/// Point coloré + texte (pour les listes d'analyse).
pub fn dot_row(ui: &mut Ui, color: Color32, title: &str, detail: &str) {
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(14.0, 20.0), Sense::hover());
        ui.painter()
            .circle_filled(rect.center() + vec2(0.0, 1.0), 4.5, color);
        ui.vertical(|ui| {
            ui.add(egui::Label::new(RichText::new(title).strong()).wrap());
            if !detail.is_empty() {
                ui.add(egui::Label::new(muted(detail).size(13.5)).wrap());
            }
        });
    });
}

/// Interrupteur animé.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let size = vec2(42.0, 24.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let bg = lerp_color(Color32::from_rgb(44, 52, 68), ACCENT_DARK, t);
    ui.painter().rect_filled(rect, CornerRadius::same(12), bg);
    let x = egui::lerp((rect.left() + 12.0)..=(rect.right() - 12.0), t);
    ui.painter()
        .circle_filled(pos2(x, rect.center().y), 9.0, Color32::WHITE);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

/// Ligne « titre + description + interrupteur ».
pub fn perm_row(
    ui: &mut Ui,
    on: &mut bool,
    title: &str,
    desc: &str,
    risk: Option<Severity>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 56.0).max(120.0));
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).strong());
                if let Some(r) = risk {
                    badge(
                        ui,
                        if r == Severity::Warning {
                            "sensible"
                        } else {
                            "à noter"
                        },
                        severity_color(r),
                    );
                }
            });
            ui.add(egui::Label::new(muted(desc).size(13.0)).wrap());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if toggle(ui, on).changed() {
                changed = true;
            }
        });
    });
    changed
}

#[derive(Clone, Copy)]
pub enum IconKind {
    Package,
    Install,
    Bolt,
}

pub fn paint_icon(p: &egui::Painter, kind: IconKind, c: Pos2, r: f32, color: Color32) {
    let st = Stroke::new(2.4, color);
    match kind {
        IconKind::Package => {
            let pts: Vec<Pos2> = (0..6)
                .map(|k| {
                    let a = (-90.0 + 60.0 * k as f32).to_radians();
                    pos2(c.x + r * a.cos(), c.y + r * a.sin())
                })
                .collect();
            p.add(Shape::closed_line(pts.clone(), st));
            for i in [1usize, 3, 5] {
                // Arêtes depuis le centre : bas, haut-droite, haut-gauche
                let target = match i {
                    1 => pts[1],
                    3 => pts[3],
                    _ => pts[5],
                };
                p.line_segment([c, target], st);
            }
        }
        IconKind::Install => {
            let top = c.y - r;
            let mid = c.y + r * 0.35;
            p.line_segment([pos2(c.x, top), pos2(c.x, mid)], st);
            p.add(Shape::line(
                vec![
                    pos2(c.x - r * 0.5, mid - r * 0.5),
                    pos2(c.x, mid),
                    pos2(c.x + r * 0.5, mid - r * 0.5),
                ],
                st,
            ));
            p.add(Shape::line(
                vec![
                    pos2(c.x - r, c.y + r * 0.15),
                    pos2(c.x - r, c.y + r),
                    pos2(c.x + r, c.y + r),
                    pos2(c.x + r, c.y + r * 0.15),
                ],
                st,
            ));
        }
        IconKind::Bolt => {
            let pts = vec![
                pos2(c.x + r * 0.25, c.y - r),
                pos2(c.x - r * 0.6, c.y + r * 0.1),
                pos2(c.x - r * 0.05, c.y + r * 0.1),
                pos2(c.x - r * 0.25, c.y + r),
                pos2(c.x + r * 0.6, c.y - r * 0.15),
                pos2(c.x + r * 0.05, c.y - r * 0.15),
            ];
            p.add(Shape::convex_polygon(pts, color.gamma_multiply(0.25), st));
        }
    }
}

pub fn paint_check(p: &egui::Painter, c: Pos2, s: f32, color: Color32, w: f32) {
    p.add(Shape::line(
        vec![
            pos2(c.x - s * 0.9, c.y),
            pos2(c.x - s * 0.25, c.y + s * 0.65),
            pos2(c.x + s * 0.95, c.y - s * 0.6),
        ],
        Stroke::new(w, color),
    ));
}

pub fn paint_cross(p: &egui::Painter, c: Pos2, s: f32, color: Color32, w: f32) {
    p.line_segment(
        [pos2(c.x - s, c.y - s), pos2(c.x + s, c.y + s)],
        Stroke::new(w, color),
    );
    p.line_segment(
        [pos2(c.x + s, c.y - s), pos2(c.x - s, c.y + s)],
        Stroke::new(w, color),
    );
}

/// Grande carte d'action cliquable de l'accueil.
pub fn action_card(
    ui: &mut Ui,
    width: f32,
    kind: IconKind,
    title: &str,
    desc: &str,
    tint: Color32,
    enabled: bool,
) -> Response {
    let size = vec2(width, 196.0);
    let (rect, resp) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered() && enabled, 0.15);
    let p = ui.painter();
    let fill = lerp_color(CARD, CARD_HOVER, t);
    let border = lerp_color(BORDER, tint, t * 0.9);
    let rect_draw = rect.translate(vec2(0.0, -2.0 * t));
    p.rect_filled(rect_draw, CornerRadius::same(16), fill);
    p.rect_stroke(
        rect_draw,
        CornerRadius::same(16),
        Stroke::new(1.0 + t * 0.6, border),
        StrokeKind::Inside,
    );
    let tile = Rect::from_min_size(rect_draw.min + vec2(22.0, 22.0), vec2(56.0, 56.0));
    p.rect_filled(
        tile,
        CornerRadius::same(14),
        tint.gamma_multiply(0.16 + 0.1 * t),
    );
    paint_icon(p, kind, tile.center(), 14.0, tint);
    p.text(
        rect_draw.min + vec2(22.0, 98.0),
        Align2::LEFT_TOP,
        title,
        FontId::new(18.5, FontFamily::Proportional),
        TEXT,
    );
    let galley = p.layout(
        desc.to_string(),
        FontId::new(13.5, FontFamily::Proportional),
        MUTED,
        width - 44.0,
    );
    p.galley(rect_draw.min + vec2(22.0, 128.0), galley, MUTED);
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Barre de progression arrondie avec animation.
pub fn progress_bar(ui: &mut Ui, value: f32, height: f32) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, height), Sense::hover());
    let v = ui
        .ctx()
        .animate_value_with_time(resp.id, value.clamp(0.0, 1.0), 0.35);
    let p = ui.painter();
    p.rect_filled(
        rect,
        CornerRadius::same((height / 2.0) as u8),
        Color32::from_rgb(30, 36, 48),
    );
    if v > 0.001 {
        let mut fill = rect;
        fill.set_width((rect.width() * v).max(height));
        p.rect_filled(fill, CornerRadius::same((height / 2.0) as u8), ACCENT);
        // reflet
        let mut hl = fill.shrink2(vec2(height * 0.4, 0.0));
        hl.set_height(height * 0.35);
        hl = hl.translate(vec2(0.0, height * 0.18));
        p.rect_filled(
            hl,
            CornerRadius::same((height / 4.0) as u8),
            Color32::from_white_alpha(40),
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Done,
    Active,
    Pending,
}

/// Indicateur d'étapes horizontal.
pub fn stepper(ui: &mut Ui, steps: &[(&str, StepState)], failed: bool) {
    let w = ui.available_width();
    let h = 62.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let n = steps.len().max(1);
    let slot = w / n as f32;
    let p = ui.painter();
    let time = ui.input(|i| i.time) as f32;
    for (i, (label, st)) in steps.iter().enumerate() {
        let cx = rect.left() + slot * (i as f32 + 0.5);
        let cy = rect.top() + 16.0;
        if i + 1 < n {
            let next_done = steps[i + 1].1 != StepState::Pending;
            let col = if *st == StepState::Done && next_done {
                ACCENT
            } else {
                BORDER
            };
            p.line_segment(
                [pos2(cx + 18.0, cy), pos2(cx + slot - 18.0, cy)],
                Stroke::new(2.0, col),
            );
        }
        let center = pos2(cx, cy);
        match st {
            StepState::Done => {
                p.circle_filled(center, 13.0, ACCENT_DARK);
                paint_check(p, center, 5.0, Color32::WHITE, 2.2);
            }
            StepState::Active => {
                let col = if failed { DANGER } else { ACCENT };
                p.circle_filled(center, 13.0, col.gamma_multiply(0.2));
                p.circle_stroke(center, 13.0, Stroke::new(2.0, col));
                if failed {
                    paint_cross(p, center, 4.5, col, 2.2);
                } else {
                    let pulse = 4.0 + 1.5 * (time * 4.0).sin();
                    p.circle_filled(center, pulse, col);
                }
            }
            StepState::Pending => {
                p.circle_stroke(center, 13.0, Stroke::new(1.5, BORDER));
            }
        }
        let col = if *st == StepState::Pending {
            MUTED
        } else {
            TEXT
        };
        p.text(
            pos2(cx, cy + 26.0),
            Align2::CENTER_TOP,
            *label,
            FontId::new(13.0, FontFamily::Proportional),
            col,
        );
    }
}

pub fn big_status_icon(ui: &mut Ui, ok: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
    let c = rect.center();
    let col = if ok { SUCCESS } else { DANGER };
    let p = ui.painter();
    p.circle_filled(c, 34.0, col.gamma_multiply(0.15));
    p.circle_stroke(c, 34.0, Stroke::new(2.5, col));
    if ok {
        paint_check(p, c, 15.0, col, 4.0);
    } else {
        paint_cross(p, c, 12.0, col, 4.0);
    }
}

pub fn single_line_trunc(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        let tail: String = s.chars().skip(n - (max - 1)).collect();
        format!("…{tail}")
    }
}
