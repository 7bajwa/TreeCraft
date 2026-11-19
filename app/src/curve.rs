//! Curves: the "volume control" beside most settings. A parent curve (green) scales a value by
//! where each node sits on its parent; a profile curve (blue) scales it along the node itself.
//! Points are (x 0..1, y multiplier); y may exceed 1 like the.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    pub pts: Vec<[f32; 2]>,
    /// Smooth (Catmull-Rom) between points instead of straight lines.
    #[serde(default)]
    pub smooth: bool,
}

impl Curve {
    pub fn flat(y: f32) -> Curve {
        Curve { pts: vec![[0.0, y], [1.0, y]], smooth: false }
    }
    pub fn from(pts: &[(f32, f32)]) -> Curve {
        let mut c = Curve { pts: pts.iter().map(|&(x, y)| [x, y]).collect(), smooth: pts.len() > 2 };
        c.sort();
        c
    }
    pub fn sort(&mut self) {
        self.pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
        if let Some(f) = self.pts.first_mut() {
            f[0] = 0.0;
        }
        if let Some(l) = self.pts.last_mut() {
            l[0] = 1.0;
        }
    }
    pub fn is_flat_one(&self) -> bool {
        self.pts.iter().all(|p| (p[1] - 1.0).abs() < 1e-6)
    }
    pub fn eval(&self, x: f32) -> f32 {
        let p = &self.pts;
        match p.len() {
            0 => 1.0,
            1 => p[0][1],
            _ => {
                let x = x.clamp(0.0, 1.0);
                let i = p.iter().position(|q| q[0] >= x).unwrap_or(p.len() - 1).max(1);
                let (a, b) = (p[i - 1], p[i]);
                let span = (b[0] - a[0]).max(1e-6);
                let u = ((x - a[0]) / span).clamp(0.0, 1.0);
                if !self.smooth {
                    return a[1] + (b[1] - a[1]) * u;
                }
                let p0 = if i >= 2 { p[i - 2][1] } else { a[1] };
                let p3 = if i + 1 < p.len() { p[i + 1][1] } else { b[1] };
                let (u2, u3) = (u * u, u * u * u);
                0.5 * (2.0 * a[1] + (-p0 + b[1]) * u + (2.0 * p0 - 5.0 * a[1] + 4.0 * b[1] - p3) * u2 + (-p0 + 3.0 * a[1] - 3.0 * b[1] + p3) * u3)
            }
        }
    }
}

pub const PRESETS: [&str; 10] = ["Flat", "Zero", "Linear up", "Linear down", "Ease in", "Ease out", "S-curve", "Bell", "Valley", "Steps"];

pub fn preset(name: &str) -> Curve {
    match name {
        "Zero" => Curve::flat(0.0),
        "Linear up" => Curve::from(&[(0.0, 0.0), (1.0, 1.0)]),
        "Linear down" => Curve::from(&[(0.0, 1.0), (1.0, 0.0)]),
        "Ease in" => Curve::from(&[(0.0, 0.0), (0.5, 0.15), (1.0, 1.0)]),
        "Ease out" => Curve::from(&[(0.0, 0.0), (0.5, 0.85), (1.0, 1.0)]),
        "S-curve" => Curve::from(&[(0.0, 0.0), (0.25, 0.1), (0.75, 0.9), (1.0, 1.0)]),
        "Bell" => Curve::from(&[(0.0, 0.2), (0.5, 1.0), (1.0, 0.2)]),
        "Valley" => Curve::from(&[(0.0, 1.0), (0.5, 0.2), (1.0, 1.0)]),
        "Steps" => Curve { pts: vec![[0.0, 0.0], [0.33, 0.0], [0.34, 0.5], [0.66, 0.5], [0.67, 1.0], [1.0, 1.0]], smooth: false },
        _ => Curve::flat(1.0),
    }
}

/// Small swatch drawn in a property row. Returns the response so a click can open the editor.
pub fn swatch(ui: &mut egui::Ui, c: &Curve, col: Color32, active: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::new(18.0, 16.0), Sense::click());
    let p = ui.painter();
    let bg = if resp.hovered() { Color32::from_gray(80) } else { Color32::from_gray(58) };
    p.rect_filled(r, CornerRadius::same(3), bg);
    if active {
        p.rect_stroke(r, CornerRadius::same(3), Stroke::new(1.0, col), egui::StrokeKind::Inside);
    }
    let top = c.pts.iter().map(|q| q[1]).fold(1.0f32, f32::max);
    let inner = r.shrink(2.5);
    let pts: Vec<Pos2> = (0..=12)
        .map(|i| {
            let x = i as f32 / 12.0;
            Pos2::new(inner.left() + x * inner.width(), inner.bottom() - (c.eval(x) / top).clamp(0.0, 1.0) * inner.height())
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.3, col)));
    resp
}

pub struct EditState {
    pub selected: Option<usize>,
    pub dragging: bool,
}

/// The Curve Editor body. Returns true if the curve changed.
/// Draws the curve body. Returns (changed, close-requested).
pub fn editor(ui: &mut egui::Ui, c: &mut Curve, st: &mut EditState, col: Color32, value: f32, default: &Curve) -> (bool, bool) {
    let mut changed = false;
    let mut close = false;
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("curve_preset").width(74.0).selected_text("Presets").show_ui(ui, |ui| {
            for p in PRESETS {
                if ui.selectable_label(false, p).clicked() {
                    *c = preset(p);
                    st.selected = None;
                    changed = true;
                }
            }
        });
        changed |= ui.checkbox(&mut c.smooth, "Smooth").changed();
        if let Some(i) = st.selected.filter(|&i| i < c.pts.len()) {
            ui.label("X");
            let end = i == 0 || i + 1 == c.pts.len();
            changed |= ui.add_enabled(!end, egui::DragValue::new(&mut c.pts[i][0]).speed(0.005).range(0.0..=1.0).max_decimals(3)).changed();
            ui.label("Y");
            changed |= ui.add(egui::DragValue::new(&mut c.pts[i][1]).speed(0.005).range(0.0..=4.0).max_decimals(3)).changed();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("✕").on_hover_text("Close").clicked() {
                close = true;
            }
            if ui.button("⟲").on_hover_text("Reset to default").clicked() {
                *c = default.clone();
                st.selected = None;
                changed = true;
            }
        });
    });
    let top = c.pts.iter().map(|q| q[1]).fold(1.0f32, f32::max).max(1.0) * 1.1;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(260.0), 190.0), Sense::click_and_drag());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, CornerRadius::same(4), Color32::from_gray(38));
    let area = rect.shrink(10.0);
    let unit = Rect::from_min_max(Pos2::new(area.left(), area.bottom() - area.height() / top), area.max);
    p.rect_filled(unit, CornerRadius::ZERO, Color32::from_gray(52));
    for i in 1..4 {
        let x = area.left() + area.width() * i as f32 / 4.0;
        p.line_segment([Pos2::new(x, area.top()), Pos2::new(x, area.bottom())], Stroke::new(1.0, Color32::from_gray(60)));
    }
    let to = |q: [f32; 2]| Pos2::new(area.left() + q[0] * area.width(), area.bottom() - q[1] / top * area.height());
    let from = |s: Pos2| [((s.x - area.left()) / area.width()).clamp(0.0, 1.0), ((area.bottom() - s.y) / area.height() * top).clamp(0.0, 4.0)];
    let line: Vec<Pos2> = (0..=80).map(|i| to([i as f32 / 80.0, c.eval(i as f32 / 80.0)])).collect();
    p.add(egui::Shape::line(line, Stroke::new(2.0, col)));
    for (i, q) in c.pts.iter().enumerate() {
        let sel = st.selected == Some(i);
        p.circle(to(*q), if sel { 5.5 } else { 4.5 }, if sel { Color32::WHITE } else { col }, Stroke::new(1.0, Color32::BLACK));
    }
    p.text(rect.left_top() + Vec2::new(6.0, 4.0), egui::Align2::LEFT_TOP, format!("× {value:.3}"), egui::FontId::proportional(11.0), Color32::from_gray(170));
    if let Some(pos) = resp.interact_pointer_pos() {
        let hit = c.pts.iter().position(|q| to(*q).distance(pos) < 9.0);
        // Double-click a point deletes it (not an endpoint).
        if resp.double_clicked() {
            if let Some(i) = hit.filter(|&i| i > 0 && i + 1 < c.pts.len()) {
                c.pts.remove(i);
                st.selected = None;
                st.dragging = false;
                changed = true;
            }
        } else if resp.drag_started() || resp.clicked() {
            match hit {
                Some(i) => {
                    st.selected = Some(i);
                    st.dragging = true;
                }
                None => {
                    // Click on empty space adds a point; a drag that starts empty just deselects.
                    if resp.clicked() {
                        let q = from(pos);
                        c.pts.push([q[0], q[1]]);
                        c.sort();
                        st.selected = c.pts.iter().position(|p| (p[0] - q[0]).abs() < 1e-6);
                        changed = true;
                    } else {
                        st.selected = None;
                    }
                    st.dragging = false;
                }
            }
        }
        if resp.dragged() && st.dragging {
            if let Some(i) = st.selected.filter(|&i| i < c.pts.len()) {
                let q = from(pos);
                let n = c.pts.len();
                let lo = if i == 0 { 0.0 } else { c.pts[i - 1][0] + 0.001 };
                let hi = if i + 1 == n { 1.0 } else { c.pts[i + 1][0] - 0.001 };
                c.pts[i] = [if i == 0 || i + 1 == n { c.pts[i][0] } else { q[0].clamp(lo, hi) }, q[1]];
                changed = true;
            }
        }
    }
    // Right-click a point: smooth/sharp (whole curve) or delete.
    // shortcut: per-point smoothing isn't stored yet; this toggles the whole-curve Smooth flag.
    let menu_hit = resp.hover_pos().and_then(|h| c.pts.iter().position(|q| to(*q).distance(h) < 10.0));
    resp.context_menu(|ui| {
        if let Some(i) = menu_hit {
            if ui.button(if c.smooth { "Make sharp" } else { "Make smooth" }).clicked() {
                c.smooth = !c.smooth;
                changed = true;
                ui.close();
            }
            if ui.add_enabled(i > 0 && i + 1 < c.pts.len(), egui::Button::new("Delete point")).clicked() {
                c.pts.remove(i);
                st.selected = None;
                changed = true;
                ui.close();
            }
        } else {
            ui.label(egui::RichText::new("Right-click a point to smooth or delete it").weak());
        }
    });
    let del = ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    if del && !ui.ctx().egui_wants_keyboard_input() {
        if let Some(i) = st.selected.filter(|&i| i > 0 && i + 1 < c.pts.len()) {
            c.pts.remove(i);
            st.selected = None;
            changed = true;
        }
    }
    ui.label(egui::RichText::new("Click empty space to add · drag to move · double-click a point to delete · right-click to smooth").size(11.0).weak());
    (changed, close)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linear_and_flat() {
        let c = Curve::from(&[(0.0, 1.0), (1.0, 0.243)]);
        assert!((c.eval(0.0) - 1.0).abs() < 1e-6 && (c.eval(1.0) - 0.243).abs() < 1e-6);
        assert!((c.eval(0.5) - 0.6215).abs() < 1e-4);
        assert!(Curve::flat(1.0).is_flat_one());
        let s = Curve::from(&[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)]);
        assert!((s.eval(0.5) - 1.0).abs() < 1e-5);
    }
}
