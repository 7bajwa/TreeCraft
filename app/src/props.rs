//! The Properties panel: a Blender-style vertical tab strip, collapsible panels and slider
//! fields, laid out from spec.rs. Every row reads and writes the generator's value by key;
//! variance and curve buttons sit beside the values that have them.

use crate::curve::{self, Curve};
use crate::model::{Gen, Kind, Mode, Pv};
use crate::spec::{self, Item, Row, RowKind};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Vec2};

pub const PARENT_COL: Color32 = Color32::from_rgb(110, 200, 95);
pub const PROFILE_COL: Color32 = Color32::from_rgb(90, 170, 235);
const VAR_COL: Color32 = Color32::from_rgb(196, 92, 140);

#[derive(Default)]
pub struct PropState {
    /// Selected tab per generator kind ("All" when None).
    pub tab: [Option<&'static str>; 4],
    pub filter: String,
    /// A curve someone asked to edit: (generator, key, profile).
    pub open_curve: Option<(u32, String, bool)>,
}

pub struct Ctx<'a> {
    pub accent: Color32,
    pub materials: &'a [String],
    pub meshes: &'a [String],
}

fn fmt(v: f32, int: bool) -> String {
    if int {
        return format!("{}", v.round() as i64);
    }
    let a = v.abs();
    if a >= 1000.0 {
        format!("{v:.0}")
    } else if a >= 100.0 {
        format!("{v:.1}")
    } else if a >= 10.0 {
        format!("{v:.2}")
    } else {
        let s = format!("{v:.3}");
        let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        if s == "-0" { "0".into() } else { s }
    }
}

/// Blender-style number field: a filled bar showing where the value sits in its range. Drag to
/// change (Shift for fine steps), click to type, right-click to reset.
pub fn slider_field(ui: &mut egui::Ui, id: egui::Id, v: &mut f32, lo: f32, hi: f32, int: bool, def: f32, accent: Color32, width: f32) -> bool {
    let h = 22.0;
    let editing_id = id.with("edit");
    let mut text: Option<String> = ui.data(|d| d.get_temp(editing_id));
    if let Some(buf) = text.as_mut() {
        let r = ui.add_sized([width, h], egui::TextEdit::singleline(buf).align(egui::Align2::CENTER_CENTER));
        r.request_focus();
        let done = r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter));
        let mut changed = false;
        if done {
            if let Ok(nv) = buf.trim().parse::<f32>() {
                let nv = if int { nv.round() } else { nv };
                changed = nv != *v;
                *v = nv;
            }
            ui.data_mut(|d| d.remove::<String>(editing_id));
        } else {
            ui.data_mut(|d| d.insert_temp(editing_id, buf.clone()));
        }
        return changed;
    }
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, h), Sense::click_and_drag());
    let p = ui.painter();
    let hovered = resp.hovered() || resp.dragged();
    p.rect_filled(rect, CornerRadius::same(5), if hovered { Color32::from_gray(96) } else { Color32::from_gray(84) });
    let span = (hi - lo).max(1e-6);
    let frac = ((*v - lo) / span).clamp(0.0, 1.0);
    if frac > 0.0 {
        let fill = Rect::from_min_size(rect.min, Vec2::new(rect.width() * frac, h));
        p.rect_filled(fill, CornerRadius { nw: 5, sw: 5, ne: if frac > 0.98 { 5 } else { 0 }, se: if frac > 0.98 { 5 } else { 0 } }, accent.gamma_multiply(if hovered { 0.85 } else { 0.7 }));
    }
    p.text(rect.center(), egui::Align2::CENTER_CENTER, fmt(*v, int), egui::FontId::proportional(13.0), Color32::from_gray(238));
    let mut changed = false;
    if resp.dragged() {
        let dx = resp.drag_delta().x;
        if dx != 0.0 {
            let fine = if ui.input(|i| i.modifiers.shift) { 0.1 } else { 1.0 };
            let mut nv = *v + dx * span / rect.width() * fine;
            nv = nv.clamp(lo.min(*v), hi.max(*v));
            if int {
                // Integers accumulate sub-step drags.
                let acc_id = id.with("acc");
                let acc: f32 = ui.data(|d| d.get_temp(acc_id)).unwrap_or(*v);
                let acc = (acc + dx * span / rect.width() * fine).clamp(lo.min(*v), hi.max(*v));
                ui.data_mut(|d| d.insert_temp(acc_id, acc));
                nv = acc.round();
            }
            changed = nv != *v;
            *v = nv;
        }
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.remove::<f32>(id.with("acc")));
    }
    if resp.clicked() {
        ui.data_mut(|d| d.insert_temp(editing_id, fmt(*v, int)));
    }
    resp.context_menu(|ui| {
        if ui.button(format!("Reset to default ({})", fmt(def, int))).clicked() {
            changed = *v != def;
            *v = def;
            ui.close();
        }
        if ui.button("Copy value").clicked() {
            ui.ctx().copy_text(fmt(*v, int));
            ui.close();
        }
    });
    changed
}

fn variance_button(ui: &mut egui::Ui, p: &mut Pv) -> bool {
    let has = p.var != 0.0;
    let label = if has { fmt(p.var, false) } else { "±".into() };
    let btn = egui::Button::new(RichText::new(label).size(11.5).color(if has { Color32::WHITE } else { Color32::from_gray(150) }))
        .fill(if has { VAR_COL.gamma_multiply(0.8) } else { Color32::from_gray(64) })
        .corner_radius(4)
        .min_size(Vec2::new(36.0, 22.0));
    let resp = ui.add_sized([36.0, 22.0], btn).on_hover_text("Variance: each node gets a random amount up to this much more or less. Click to edit.");
    let mut changed = false;
    egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(150.0);
        ui.horizontal(|ui| {
            changed |= ui.add(egui::DragValue::new(&mut p.var).speed(0.005).max_decimals(4)).changed();
            egui::ComboBox::from_id_salt("var_dist").width(86.0).selected_text(if p.gauss { "Gaussian" } else { "Uniform" }).show_ui(ui, |ui| {
                changed |= ui.selectable_value(&mut p.gauss, false, "Uniform").changed();
                changed |= ui.selectable_value(&mut p.gauss, true, "Gaussian").changed();
            });
            if ui.button("⟲").on_hover_text("Clear variance").clicked() {
                p.var = 0.0;
                changed = true;
            }
        });
    });
    changed
}

fn label_cell(ui: &mut egui::Ui, w: f32, text: &str, row: &Row) {
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 22.0), Sense::hover());
    let col = if row.inert { Color32::from_gray(120) } else { Color32::from_gray(214) };
    let galley = ui.fonts_mut(|f| f.layout_no_wrap(text.to_string(), egui::FontId::proportional(13.0), col));
    let pos = Pos2::new(r.right() - 8.0 - galley.size().x.min(w - 8.0), r.center().y - galley.size().y / 2.0);
    ui.painter_at(r).galley(pos, galley, col);
    let mut tip = row.tip.clone();
    if row.inert {
        tip = format!("{}{}This setting is saved but doesn't change the model yet (see the settings list).", tip, if tip.is_empty() { "" } else { "\n\n" });
    }
    if !tip.is_empty() {
        resp.on_hover_text(format!("{tip}\n\nKey: {}", row.key));
    }
}

/// One row. Returns (changed, curve the person clicked).
fn row_ui(ui: &mut egui::Ui, g: &mut Gen, row: &Row, c: &Ctx) -> (bool, Option<bool>) {
    let mut changed = false;
    let mut open = None;
    let id = ui.make_persistent_id(("row", g.id, &row.key));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let total = ui.available_width();
        let label_w = (total * 0.42).clamp(96.0, 170.0);
        label_cell(ui, label_w, &row.label, row);
        let extras = (if row.var { 40.0 } else { 0.0 }) + if row.parent { 22.0 } else { 0.0 } + if row.profile && row.kind != RowKind::Curve { 22.0 } else { 0.0 };
        // Extras plus the 4 px gap before each, so rows never push the panel wider.
        let w = (ui.available_width() - extras - 14.0).max(50.0);
        match &row.kind {
            RowKind::Num { int } => {
                let mut p = g.pv(&row.key);
                changed |= slider_field(ui, id, &mut p.v, row.lo, row.hi, *int, row.def, c.accent, w);
                if row.var {
                    changed |= variance_button(ui, &mut p);
                }
                if changed {
                    g.vals.insert(row.key.clone(), p);
                }
            }
            RowKind::Check => {
                let mut b = g.b(&row.key);
                if ui.add_sized([w.min(24.0), 22.0], egui::Checkbox::without_text(&mut b)).changed() {
                    g.set(&row.key, b as u8 as f32);
                    changed = true;
                }
                ui.add_space((w - 24.0).max(0.0));
            }
            RowKind::Combo(opts) => {
                let mut i = g.idx(&row.key).min(opts.len().saturating_sub(1));
                egui::ComboBox::from_id_salt(id).width((w - 30.0).max(40.0)).selected_text(opts.get(i).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                    for (k, o) in opts.iter().enumerate() {
                        if ui.selectable_value(&mut i, k, o).changed() {
                            changed = true;
                        }
                    }
                });
                if changed {
                    g.set(&row.key, i as f32);
                }
            }
            RowKind::Mode => {
                let mut m = g.mode();
                egui::ComboBox::from_id_salt(id).width((w - 30.0).max(40.0)).selected_text(m.label()).show_ui(ui, |ui| {
                    for mm in Mode::ALL {
                        changed |= ui.selectable_value(&mut m, mm, mm.label()).changed();
                    }
                });
                if changed {
                    g.set_mode(m);
                }
            }
            RowKind::Mat | RowKind::Mesh => {
                let (names, cur): (&[String], &mut Option<usize>) = if row.kind == RowKind::Mat { (c.materials, &mut g.material) } else { (c.meshes, &mut g.mesh) };
                let none = if row.kind == RowKind::Mat { "Inherited" } else { "From material" };
                let text = cur.and_then(|i| names.get(i).cloned()).unwrap_or_else(|| none.to_string());
                egui::ComboBox::from_id_salt(id).width((w - 30.0).max(40.0)).selected_text(text).show_ui(ui, |ui| {
                    changed |= ui.selectable_value(cur, None, none).changed();
                    for (k, n) in names.iter().enumerate() {
                        changed |= ui.selectable_value(cur, Some(k), n).changed();
                    }
                });
            }
            RowKind::Color => {
                changed |= ui.color_edit_button_srgb(&mut g.color).changed();
            }
            RowKind::Seed => {
                changed |= ui.add_sized([(w - 40.0).max(40.0), 22.0], egui::DragValue::new(&mut g.seed)).changed();
                if ui.button("🎲").on_hover_text("New random seed").clicked() {
                    g.seed = g.seed.wrapping_mul(2_654_435_761).wrapping_add(97);
                    changed = true;
                }
            }
            RowKind::Text => {
                let t = g.texts.entry(row.key.clone()).or_default();
                changed |= ui.add_sized([w, 22.0], egui::TextEdit::singleline(t)).changed();
            }
            RowKind::Curve => {
                let profile = !row.parent;
                let cv = g.curve(&row.key, profile).unwrap_or_else(|| Curve::flat(1.0));
                let col = if profile { PROFILE_COL } else { PARENT_COL };
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w.min(110.0), 22.0), Sense::click());
                let p = ui.painter();
                p.rect_filled(r, CornerRadius::same(5), if resp.hovered() { Color32::from_gray(80) } else { Color32::from_gray(62) });
                let inner = r.shrink2(Vec2::new(6.0, 4.0));
                let top = cv.pts.iter().map(|q| q[1]).fold(1.0f32, f32::max);
                let line: Vec<Pos2> = (0..=24).map(|i| {
                    let x = i as f32 / 24.0;
                    Pos2::new(inner.left() + x * inner.width(), inner.bottom() - (cv.eval(x) / top).clamp(0.0, 1.0) * inner.height())
                }).collect();
                p.add(egui::Shape::line(line, Stroke::new(1.5, col)));
                if resp.on_hover_text("Click to edit the curve").clicked() {
                    open = Some(profile);
                }
            }
        }
        if row.parent && row.kind != RowKind::Curve {
            let cv = g.curve(&row.key, false).unwrap_or_else(|| Curve::flat(1.0));
            let edited = g.curves.contains_key(&format!("{}|p", row.key)) || !cv.is_flat_one();
            if curve::swatch(ui, &cv, PARENT_COL, edited).on_hover_text("Parent curve: scales this value by where each node sits on its parent").clicked() {
                open = Some(false);
            }
        }
        if row.profile && row.kind != RowKind::Curve {
            let cv = g.curve(&row.key, true).unwrap_or_else(|| Curve::flat(1.0));
            let edited = g.curves.contains_key(&format!("{}|f", row.key)) || !cv.is_flat_one();
            if curve::swatch(ui, &cv, PROFILE_COL, edited).on_hover_text("Profile curve: scales this value from the base (left) to the tip (right) of each node").clicked() {
                open = Some(true);
            }
        }
    });
    (changed, open)
}

fn panel(ui: &mut egui::Ui, id: egui::Id, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
    let open = state.is_open();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
    let p = ui.painter();
    let rounding = if open { CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 } } else { CornerRadius::same(6) };
    p.rect_filled(rect, rounding, if resp.hovered() { Color32::from_gray(66) } else { Color32::from_gray(58) });
    // The font has no chevron glyphs, so draw the triangle.
    let c = rect.left_center() + Vec2::new(15.0, 0.0);
    let tri = if open { vec![c + Vec2::new(-4.5, -2.5), c + Vec2::new(4.5, -2.5), c + Vec2::new(0.0, 3.5)] } else { vec![c + Vec2::new(-2.5, -4.5), c + Vec2::new(3.5, 0.0), c + Vec2::new(-2.5, 4.5)] };
    p.add(egui::Shape::convex_polygon(tri, Color32::from_gray(190), Stroke::NONE));
    p.text(rect.left_center() + Vec2::new(28.0, 0.0), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(14.0), Color32::from_gray(235));
    if resp.clicked() {
        let mut s = state;
        s.toggle(ui);
        s.store(ui.ctx());
    }
    if open {
        egui::Frame::new().fill(Color32::from_gray(48)).corner_radius(CornerRadius { nw: 0, ne: 0, sw: 6, se: 6 }).inner_margin(egui::Margin { left: 6, right: 6, top: 6, bottom: 8 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 3.0;
            add(ui);
        });
    }
    ui.add_space(6.0);
}

/// Sections of one tab (or of every tab when searching). Returns true if anything changed;
/// a curve click lands in `st.open_curve`.
fn sections(ui: &mut egui::Ui, g: &mut Gen, tab_list: &[&spec::Tab], st: &mut PropState, c: &Ctx) -> bool {
    let mut changed = false;
    let filter = st.filter.trim().to_lowercase();
    let mode = g.mode().label();
    for tab in tab_list {
        for (si, sec) in tab.sections.iter().enumerate() {
            // Visible items: hide other modes' groups; filter by label or key.
            let mut vis: Vec<&Item> = vec![];
            let mut skip_group = false;
            for item in &sec.items {
                match item {
                    Item::Group(name) => {
                        skip_group = sec.title == "Generation" && spec::MODE_GROUPS.contains(&name.as_str()) && name != mode;
                        if !skip_group {
                            vis.push(item);
                        }
                    }
                    Item::Sub(_) => {
                        if !skip_group {
                            vis.push(item);
                        }
                    }
                    Item::Row(r) => {
                        let in_mode_group = sec.title == "Generation" && r.key.starts_with("Generation:") && skip_group;
                        if in_mode_group {
                            continue;
                        }
                        if filter.is_empty() || r.label.to_lowercase().contains(&filter) || r.key.to_lowercase().contains(&filter) || sec.title.to_lowercase().contains(&filter) {
                            vis.push(item);
                        }
                    }
                }
            }
            if !vis.iter().any(|i| matches!(i, Item::Row(_))) {
                continue;
            }
            // Drop headings with nothing under them.
            let mut kept: Vec<&Item> = vec![];
            for (k, item) in vis.iter().enumerate() {
                let next_row = vis[k + 1..].iter().find(|i| !matches!(i, Item::Sub(_)) || matches!(item, Item::Group(_)));
                match item {
                    Item::Group(_) => {
                        if vis[k + 1..].iter().take_while(|i| !matches!(i, Item::Group(_))).any(|i| matches!(i, Item::Row(_))) {
                            kept.push(item);
                        }
                    }
                    Item::Sub(_) => {
                        if matches!(next_row, Some(Item::Row(_))) {
                            kept.push(item);
                        }
                    }
                    Item::Row(_) => kept.push(item),
                }
            }
            let pid = ui.make_persistent_id(("panel", g.kind as u8, tab.name, si));
            panel(ui, pid, &sec.title, |ui| {
                for item in kept {
                    match item {
                        Item::Group(name) => {
                            ui.add_space(5.0);
                            ui.label(RichText::new(name).size(13.0).strong().color(Color32::from_gray(200)));
                        }
                        Item::Sub(name) => {
                            ui.add_space(3.0);
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(RichText::new(name).size(12.0).italics().color(Color32::from_gray(160)));
                            });
                        }
                        Item::Row(r) => {
                            let (ch, open) = row_ui(ui, g, r, c);
                            changed |= ch;
                            if let Some(profile) = open {
                                st.open_curve = Some((g.id, r.key.clone(), profile));
                            }
                        }
                    }
                }
            });
        }
    }
    changed
}

/// The whole properties area for a generator: tab strip, title, search, panels.
pub fn show(ui: &mut egui::Ui, g: &mut Gen, st: &mut PropState, c: &Ctx) -> bool {
    let tabs = spec::tabs(g.kind);
    let slot = g.kind.spec_base() as usize;
    let mut changed = false;
    ui.horizontal_top(|ui| {
        // Vertical icon tabs, like Blender's Properties editor.
        ui.vertical(|ui| {
            ui.set_width(34.0);
            ui.spacing_mut().item_spacing.y = 3.0;
            let tab_btn = |ui: &mut egui::Ui, icon: &str, name: &'static str, sel: bool| -> bool {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(32.0, 32.0), Sense::click());
                let p = ui.painter();
                let fill = if sel { c.accent } else if resp.hovered() { Color32::from_gray(70) } else { Color32::from_gray(52) };
                p.rect_filled(r, CornerRadius::same(6), fill);
                if icon == "☰" {
                    for k in -1..=1 {
                        let y = r.center().y + k as f32 * 5.0;
                        p.line_segment([Pos2::new(r.center().x - 7.0, y), Pos2::new(r.center().x + 7.0, y)], Stroke::new(2.0, Color32::WHITE));
                    }
                } else {
                    p.text(r.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(16.0), Color32::WHITE);
                }
                resp.on_hover_text(name).clicked()
            };
            if tabs.len() > 1 && tab_btn(ui, "☰", "All", st.tab[slot].is_none()) {
                st.tab[slot] = None;
            }
            for t in tabs {
                if tab_btn(ui, t.icon, t.name, st.tab[slot] == Some(t.name) || tabs.len() == 1) {
                    st.tab[slot] = Some(t.name);
                }
            }
        });
        ui.vertical(|ui| {
            let current: Vec<&spec::Tab> = if !st.filter.trim().is_empty() || tabs.len() == 1 {
                tabs.iter().collect()
            } else {
                match st.tab[slot] {
                    Some(name) => tabs.iter().filter(|t| t.name == name).collect(),
                    None => tabs.iter().collect(),
                }
            };
            let title = if !st.filter.trim().is_empty() { "Search results" } else { st.tab[slot].filter(|_| tabs.len() > 1).unwrap_or(if tabs.len() == 1 { tabs[0].name } else { "All" }) };
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).size(15.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut st.filter).hint_text("🔍 Search settings").desired_width((ui.available_width() - 8.0).clamp(80.0, 200.0)));
                });
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical().id_salt(("props_scroll", g.kind as u8)).auto_shrink([false, false]).show(ui, |ui| {
                changed |= sections(ui, g, &current, st, c);
            });
        });
    });
    changed
}

/// Settings that are shown and saved but don't change the model yet, as text for the person.
pub fn inert_list() -> String {
    let mut s = String::new();
    for (kind, title) in [(Kind::Tree, "Tree node"), (Kind::Branch, "Branch generator"), (Kind::Leaf, "Leaf generator"), (Kind::Frond, "Frond generator")] {
        s.push_str(&format!("\n=== {title} ===\n"));
        for tab in spec::tabs(kind) {
            for sec in &tab.sections {
                let (mut grp, mut sub) = (String::new(), String::new());
                for item in &sec.items {
                    match item {
                        Item::Group(n) => {
                            grp = n.clone();
                            sub.clear();
                        }
                        Item::Sub(n) => sub = n.clone(),
                        Item::Row(r) if r.inert => {
                            let path = [tab.name, sec.title.as_str(), grp.as_str(), sub.as_str(), r.label.as_str()].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" > ");
                            s.push_str(&format!("- {path}    [{}]\n", r.key));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    s
}
