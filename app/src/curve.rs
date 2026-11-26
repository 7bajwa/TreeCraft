//! Curves: the "volume control" beside most settings. A parent curve (green) scales a value by
//! where each node sits on its parent; a profile curve (blue) scales it along the node itself.
//! Points are (x 0..1, y multiplier); y may exceed 1.
//!
//! A curve is drawn one of two ways: the whole curve straight or smooth (the `smooth` flag,
//! no `knots`), or point by point like a pen tool (`knots`, one per point), where each point is
//! smooth (its two handles stay in line) or a corner (handles move on their own, none = sharp).

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    pub pts: Vec<[f32; 2]>,
    /// Smooth (Catmull-Rom) between points instead of straight lines, when there are no knots.
    #[serde(default)]
    pub smooth: bool,
    /// Per-point handles (pen tool), one per point; empty until a point is edited that way.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knots: Vec<Knot>,
}

/// One point's handles, as offsets from the point: `hin` toward the previous point, `hout`
/// toward the next.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Knot {
    pub smooth: bool,
    pub hin: [f32; 2],
    pub hout: [f32; 2],
}

impl Curve {
    pub fn flat(y: f32) -> Curve {
        // Smooth by default, so a point added later bends the curve rather than making a corner.
        Curve { pts: vec![[0.0, y], [1.0, y]], smooth: true, knots: vec![] }
    }
    pub fn from(pts: &[(f32, f32)]) -> Curve {
        let mut c = Curve { pts: pts.iter().map(|&(x, y)| [x, y]).collect(), smooth: pts.len() > 2, knots: vec![] };
        c.sort();
        c
    }
    pub fn has_knots(&self) -> bool {
        !self.knots.is_empty() && self.knots.len() == self.pts.len()
    }
    pub fn sort(&mut self) {
        if self.has_knots() {
            let mut both: Vec<([f32; 2], Knot)> = self.pts.iter().copied().zip(self.knots.iter().copied()).collect();
            both.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
            (self.pts, self.knots) = both.into_iter().unzip();
        } else {
            self.knots.clear();
            self.pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
        }
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

    /// Adds a point where it belongs and returns its index. With knots it gets handles of the
    /// curve's kind (smooth or sharp).
    pub fn add_point(&mut self, q: [f32; 2]) -> usize {
        let n = self.pts.len();
        // Between the end points, which keep x = 0 and 1.
        let i = if n < 2 { n } else { self.pts.iter().position(|p| p[0] > q[0]).unwrap_or(n).clamp(1, n - 1) };
        let knots = self.has_knots();
        self.pts.insert(i, q);
        if knots {
            self.knots.insert(i, Knot { smooth: false, hin: [0.0; 2], hout: [0.0; 2] });
            self.knots[i] = self.auto_knot(i, self.smooth);
        }
        i
    }
    pub fn remove_point(&mut self, i: usize) {
        if i >= self.pts.len() {
            return;
        }
        let knots = self.has_knots();
        self.pts.remove(i);
        if knots {
            self.knots.remove(i);
        }
    }

    /// The handles point i has now, whether or not knots are stored: in the curve-wide mode a
    /// smooth curve's handles reproduce its Catmull-Rom shape exactly, a straight one has none.
    pub fn knot(&self, i: usize) -> Knot {
        if self.has_knots() {
            return self.knots[i];
        }
        let p = &self.pts;
        let n = p.len();
        if !self.smooth || n < 2 {
            return Knot { smooth: false, hin: [0.0; 2], hout: [0.0; 2] };
        }
        let m = (p[(i + 1).min(n - 1)][1] - p[i.saturating_sub(1)][1]) * 0.5;
        let span_in = if i > 0 { p[i][0] - p[i - 1][0] } else { 0.0 };
        let span_out = if i + 1 < n { p[i + 1][0] - p[i][0] } else { 0.0 };
        Knot { smooth: true, hin: [-span_in / 3.0, -m / 3.0], hout: [span_out / 3.0, m / 3.0] }
    }

    /// Stores per-point knots (from the current shape) so single points can be edited.
    pub fn make_knots(&mut self) {
        if !self.has_knots() {
            self.knots = (0..self.pts.len()).map(|i| self.knot(i)).collect();
        }
    }

    /// Handles for point i made from its neighbours: in one line along their slope when smooth,
    /// none when sharp.
    fn auto_knot(&self, i: usize, smooth: bool) -> Knot {
        let p = &self.pts;
        let n = p.len();
        if !smooth || n < 2 {
            return Knot { smooth, hin: [0.0; 2], hout: [0.0; 2] };
        }
        let (a, b) = (p[i.saturating_sub(1)], p[(i + 1).min(n - 1)]);
        let slope = (b[1] - a[1]) / (b[0] - a[0]).max(1e-6);
        let span_in = if i > 0 { p[i][0] - p[i - 1][0] } else { 0.0 } / 3.0;
        let span_out = if i + 1 < n { p[i + 1][0] - p[i][0] } else { 0.0 } / 3.0;
        Knot { smooth: true, hin: [-span_in, -slope * span_in], hout: [span_out, slope * span_out] }
    }

    /// Makes point i smooth (handles in line, from its neighbours) or sharp (no handles).
    pub fn set_smooth(&mut self, i: usize, smooth: bool) {
        if i >= self.pts.len() {
            return;
        }
        self.make_knots();
        self.knots[i] = self.auto_knot(i, smooth);
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
                if self.has_knots() {
                    // Cubic Bezier through the handles; x is solved for t (handles kept inside
                    // the segment so x only grows).
                    let (ka, kb) = (self.knots[i - 1], self.knots[i]);
                    let c1 = [a[0] + ka.hout[0].clamp(0.0, span), a[1] + ka.hout[1]];
                    let c2 = [b[0] + kb.hin[0].clamp(-span, 0.0), b[1] + kb.hin[1]];
                    let bez = |t: f32, k: usize| {
                        let s = 1.0 - t;
                        s * s * s * a[k] + 3.0 * s * s * t * c1[k] + 3.0 * s * t * t * c2[k] + t * t * t * b[k]
                    };
                    let (mut lo, mut hi) = (0.0f32, 1.0f32);
                    for _ in 0..24 {
                        let mid = (lo + hi) * 0.5;
                        if bez(mid, 0) < x {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    return bez((lo + hi) * 0.5, 1);
                }
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
        "Steps" => Curve { pts: vec![[0.0, 0.0], [0.33, 0.0], [0.34, 0.5], [0.66, 0.5], [0.67, 1.0], [1.0, 1.0]], smooth: false, knots: vec![] },
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

#[derive(Default)]
pub struct EditState {
    pub selected: Option<usize>,
    /// What the held mouse moves: 0 nothing, 1 the selected point, 2 its in handle, 3 its out handle.
    pub grab: u8,
    /// The point the right-click menu is for.
    pub menu: Option<usize>,
}

/// The Curve Editor body. Returns (changed, close-requested).
///
/// Pen-tool editing: press empty space to add a point (keep holding to drag it), press a point
/// to select and drag it, drag a selected point's handles to bend the curve, right-click or
/// Alt+click a point to switch it between smooth and corner (Alt-dragging a handle breaks it into
/// a corner), double-click or Delete removes a point.
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
        if ui.checkbox(&mut c.smooth, "Smooth").on_hover_text("Make every point smooth (on) or sharp (off); right-click a point to change only that one").changed() {
            c.knots.clear();
            changed = true;
        }
        if let Some(i) = st.selected.filter(|&i| i < c.pts.len()) {
            ui.label("X");
            let end = i == 0 || i + 1 == c.pts.len();
            changed |= ui.add_enabled(!end, egui::DragValue::new(&mut c.pts[i][0]).speed(0.005).range(0.0..=1.0).max_decimals(3)).changed();
            ui.label("Y");
            changed |= ui.add(egui::DragValue::new(&mut c.pts[i][1]).speed(0.005).range(0.0..=4.0).max_decimals(3)).changed();
            let k = c.knot(i);
            let label = if k.smooth { "◯ Smooth" } else { "◇ Corner" };
            if ui.small_button(label).on_hover_text("Switch this point between smooth and corner (Alt+click it)").clicked() {
                c.set_smooth(i, !k.smooth);
                changed = true;
            }
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
    let raw = |s: Pos2| [(s.x - area.left()) / area.width(), (area.bottom() - s.y) / area.height() * top];
    let from = |s: Pos2| {
        let q = raw(s);
        [q[0].clamp(0.0, 1.0), q[1].clamp(0.0, 4.0)]
    };
    // Handle ends of the selected point on screen: (which, position).
    let handles = |c: &Curve, i: usize| -> Vec<(u8, Pos2)> {
        let k = c.knot(i);
        let n = c.pts.len();
        let q = c.pts[i];
        let mut v = vec![];
        if i > 0 && (k.smooth || k.hin != [0.0; 2]) {
            v.push((2, to([q[0] + k.hin[0], q[1] + k.hin[1]])));
        }
        if i + 1 < n && (k.smooth || k.hout != [0.0; 2]) {
            v.push((3, to([q[0] + k.hout[0], q[1] + k.hout[1]])));
        }
        v
    };

    // Press: grab a handle, a point, or add a point (on press, not release, so one click is
    // always enough and a slow frame can't turn it into a drag that misses).
    let (pressed, alt, ptr, down) = ui.input(|i| (i.pointer.primary_pressed(), i.modifiers.alt, i.pointer.interact_pos(), i.pointer.primary_down()));
    if pressed && resp.hovered() {
        if let Some(pos) = ptr {
            let near = |s: Pos2| s.distance(pos) < 11.0;
            let handle = st.selected.filter(|&i| i < c.pts.len()).and_then(|i| handles(c, i).into_iter().find(|(_, s)| near(*s)).map(|h| h.0));
            let point = c.pts.iter().enumerate().map(|(i, q)| (i, to(*q).distance(pos))).filter(|(_, d)| *d < 11.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0);
            if let Some(h) = handle {
                st.grab = h;
                if alt {
                    let i = st.selected.unwrap();
                    c.make_knots();
                    c.knots[i].smooth = false;
                    changed = true;
                }
            } else if let Some(i) = point {
                st.selected = Some(i);
                if alt {
                    let s = c.knot(i).smooth;
                    c.set_smooth(i, !s);
                    st.grab = 0;
                    changed = true;
                } else {
                    st.grab = 1;
                }
            } else {
                let i = c.add_point(from(pos));
                st.selected = Some(i);
                st.grab = 1;
                changed = true;
            }
        }
    }
    if !down {
        st.grab = 0;
    }
    if st.grab != 0 && down {
        if let (Some(i), Some(pos)) = (st.selected.filter(|&i| i < c.pts.len()), ptr) {
            let n = c.pts.len();
            if st.grab == 1 {
                let q = from(pos);
                let lo = if i == 0 { 0.0 } else { c.pts[i - 1][0] + 0.001 };
                let hi = if i + 1 == n { 1.0 } else { c.pts[i + 1][0] - 0.001 };
                let nq = [if i == 0 || i + 1 == n { c.pts[i][0] } else { q[0].clamp(lo, hi) }, q[1]];
                if nq != c.pts[i] {
                    c.pts[i] = nq;
                    changed = true;
                }
            } else {
                c.make_knots();
                let q = raw(pos);
                let mut h = [q[0] - c.pts[i][0], q[1] - c.pts[i][1]];
                let k = &mut c.knots[i];
                let (own, other) = if st.grab == 2 {
                    h[0] = h[0].min(0.0);
                    (&mut k.hin, &mut k.hout)
                } else {
                    h[0] = h[0].max(0.0);
                    (&mut k.hout, &mut k.hin)
                };
                if *own != h {
                    *own = h;
                    if k.smooth {
                        // Keep the opposite handle in line, at its own length.
                        let (hl, ol) = ((h[0] * h[0] + h[1] * h[1]).sqrt(), (other[0] * other[0] + other[1] * other[1]).sqrt());
                        let s = if ol > 1e-5 && hl > 1e-5 { ol / hl } else { 1.0 };
                        *other = [-h[0] * s, -h[1] * s];
                    }
                    changed = true;
                }
            }
        }
    }

    // Draw: curve, the selected point's handles, then points (circles smooth, diamonds corner).
    let line: Vec<Pos2> = (0..=120).map(|i| to([i as f32 / 120.0, c.eval(i as f32 / 120.0)])).collect();
    p.add(egui::Shape::line(line, Stroke::new(2.0, col)));
    if let Some(i) = st.selected.filter(|&i| i < c.pts.len()) {
        let o = to(c.pts[i]);
        for (_, s) in handles(c, i) {
            p.line_segment([o, s], Stroke::new(1.0, Color32::from_gray(200)));
            p.circle(s, 3.5, Color32::from_gray(30), Stroke::new(1.2, Color32::WHITE));
        }
    }
    for (i, q) in c.pts.iter().enumerate() {
        let sel = st.selected == Some(i);
        let fill = if sel { Color32::WHITE } else { col };
        let s = to(*q);
        if c.knot(i).smooth {
            p.circle(s, if sel { 5.5 } else { 4.5 }, fill, Stroke::new(1.0, Color32::BLACK));
        } else {
            let r = if sel { 6.0 } else { 5.0 };
            p.add(egui::Shape::convex_polygon(vec![s + Vec2::new(0.0, -r), s + Vec2::new(r, 0.0), s + Vec2::new(0.0, r), s + Vec2::new(-r, 0.0)], fill, Stroke::new(1.0, Color32::BLACK)));
        }
    }
    p.text(rect.left_top() + Vec2::new(6.0, 4.0), egui::Align2::LEFT_TOP, format!("× {value:.3}"), egui::FontId::proportional(11.0), Color32::from_gray(170));

    // Right-click a point switches it between smooth and corner; double-click deletes it.
    if resp.secondary_clicked() {
        if let Some(i) = resp.interact_pointer_pos().and_then(|h| c.pts.iter().position(|q| to(*q).distance(h) < 11.0)) {
            let s = c.knot(i).smooth;
            c.set_smooth(i, !s);
            st.selected = Some(i);
            changed = true;
        }
    }
    if resp.double_clicked() {
        if let Some(i) = resp.interact_pointer_pos().and_then(|h| c.pts.iter().position(|q| to(*q).distance(h) < 11.0)).filter(|&i| i > 0 && i + 1 < c.pts.len()) {
            c.remove_point(i);
            st.selected = None;
            st.grab = 0;
            changed = true;
        }
    }
    let del = ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    if del && !ui.ctx().egui_wants_keyboard_input() {
        if let Some(i) = st.selected.filter(|&i| i > 0 && i + 1 < c.pts.len()) {
            c.remove_point(i);
            st.selected = None;
            changed = true;
        }
    }
    ui.label(egui::RichText::new("Click to add · drag points and handles · right-click or Alt+click: smooth ⇄ corner · double-click or Del: delete").size(11.0).weak());
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

    #[test]
    fn knots_keep_the_shape_then_edit_one_point() {
        // Turning on per-point editing must not change the curve.
        for smooth in [false, true] {
            let mut c = Curve::from(&[(0.0, 0.2), (0.3, 0.9), (0.7, 0.4), (1.0, 1.0)]);
            c.smooth = smooth;
            let before: Vec<f32> = (0..=20).map(|i| c.eval(i as f32 / 20.0)).collect();
            c.make_knots();
            for (i, b) in before.iter().enumerate() {
                assert!((c.eval(i as f32 / 20.0) - b).abs() < 2e-3, "smooth={smooth} x={i}");
            }
        }
        // A sharp point passes through itself; a smooth one there bends it.
        let mut c = Curve::from(&[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)]);
        c.smooth = false;
        c.set_smooth(1, false);
        assert!((c.eval(0.25) - 0.5).abs() < 1e-3);
        c.set_smooth(1, true);
        assert!(c.eval(0.25) > 0.55);
        // Adding and removing keep knots in step with points.
        let i = c.add_point([0.8, 0.3]);
        assert_eq!((i, c.pts.len(), c.knots.len()), (2, 4, 4));
        c.remove_point(2);
        assert_eq!(c.knots.len(), 3);
    }
}
