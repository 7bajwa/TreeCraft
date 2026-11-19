//! Right-hand editors: Materials (with picture slots), Material Sets, Meshes, Displacements, and
//! the Cutout Editor dialog. Also loads material pictures for the views and the 3D atlas.

use crate::cutout;
use crate::math::hash;
use crate::curve;
use crate::model::{map_label, Anchor, DispAsset, Doc, MapAdjust, MapSlot, Material, MaterialSet, MeshAsset, SetMember, SlotCfg, UvArea, MAP_IS_COLOR, MAP_SHOWN};
use crate::texgen::{self, MapGen, Ramp, GRADIENTS, NOISES, SOURCES};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Default)]
pub struct EditorState {
    pub tab: usize,
    pub mat: usize,
    pub mat_sub: usize,
    pub mesh: usize,
    pub mesh_show: usize,
    pub alignment: bool,
    /// The Material Sets tab's set.
    pub set: usize,
    /// Adding a UV area opens it in the Area Editor.
    pub area_on_add: bool,
    /// The Displacements tab's asset, and its preview.
    pub disp: usize,
    pub disp_tex: Option<(String, egui::TextureHandle)>,
    pub edit_mesh: Option<MeshEdit>,
    /// A command for the app ("reload", "new-cutout").
    pub message: Option<String>,
    /// A note for the person, shown in the editors.
    pub note: Option<String>,
    /// The Edit Map dialog, when open.
    pub map_edit: Option<MapEdit>,
    /// Copied map adjustments.
    pub adj_clip: Option<MapAdjust>,
    /// The UV Area editor, when open.
    pub area_edit: Option<AreaEdit>,
    /// Height of the material settings box (the maps box starts below it).
    pub mat_split: f32,
    /// A list row being dragged to a new place: (list, row).
    pub row_drag: Option<(&'static str, usize)>,
    /// How the cutout list sorts: (0 name, 1 points, 2 size; descending).
    pub cut_sort: (usize, bool),
    /// Clicking an empty map slot doesn't open the file picker.
    pub no_ask_pick: bool,
    /// The Meshes tab slot waiting on "convert or import?".
    pub import_ask: Option<usize>,
    /// How the Meshes tab's 3D view is turned (added to its starting angle).
    pub view_yaw: f32,
    pub view_pitch: f32,
}

// ---------------------------------------------------------------- pictures

/// Material pictures as textures, plus one atlas the 3D view samples leaves and fronds from.
#[derive(Default)]
pub struct Pics {
    sig: u64,
    pub color: Vec<Option<egui::TextureHandle>>,
    pub alpha: Vec<Option<egui::TextureHandle>>,
    /// The small preview picture an imported .spm file carries for each material.
    pub preview: Vec<Option<egui::TextureHandle>>,
    pub atlas: Option<egui::TextureHandle>,
    /// The atlas pixels, for the 3D view's software raster.
    pub atlas_img: Option<std::sync::Arc<image::RgbaImage>>,
    /// Per material, per map slot: a small picture for the maps grid.
    pub thumbs: Vec<Vec<Option<egui::TextureHandle>>>,
    /// Per material: uv rectangle [u0, v0, u1, v1] in the atlas.
    pub cells: Vec<Option<[f32; 4]>>,
    /// A white spot in the atlas for untextured triangles.
    pub white: [f32; 2],
}

fn load_picture(m: &Material) -> Option<image::RgbaImage> {
    let cap = match m.max_res {
        1 => 256,
        2 => 512,
        _ => 1024,
    };
    let mut img = slot_image(m.maps.first().filter(|s| s.on)?, 512)?;
    if img.width() > cap || img.height() > cap {
        img = image::imageops::resize(&img, cap, cap, image::imageops::FilterType::Triangle);
    }
    img = adjusted(&img, &m.maps[0].adj);
    if let Some(op) = m.maps.get(1).filter(|s| s.on).and_then(|s| slot_image(s, 512)) {
        let op = image::DynamicImage::ImageRgba8(adjusted(&op, &m.maps[1].adj)).to_luma8();
        let op = image::imageops::resize(&op, img.width(), img.height(), image::imageops::FilterType::Triangle);
        for (p, a) in img.pixels_mut().zip(op.pixels()) {
            p[3] = a[0];
        }
    }
    Some(img)
}

fn to_color_image(img: &image::RgbaImage, max: u32) -> egui::ColorImage {
    let img = if img.width() > max || img.height() > max { image::imageops::resize(img, max, max, image::imageops::FilterType::Triangle) } else { img.clone() };
    egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw())
}

impl Pics {
    pub fn sync(&mut self, ctx: &egui::Context, mats: &[Material]) {
        let mut h = DefaultHasher::new();
        for m in mats {
            serde_json::to_string(&(&m.maps, m.max_res, m.atlas_scale)).unwrap_or_default().hash(&mut h);
            m.preview_png.as_ref().map(|b| b.len()).hash(&mut h);
        }
        mats.len().hash(&mut h);
        let sig = h.finish();
        // While a slider is being dragged, wait: reading every picture again each frame is slow.
        if sig == self.sig && self.atlas.is_some() || self.atlas.is_some() && ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        self.sig = sig;
        let imgs: Vec<Option<image::RgbaImage>> = mats.iter().map(load_picture).collect();
        self.preview = mats
            .iter()
            .enumerate()
            .map(|(i, m)| m.preview_png.as_ref().and_then(|b| image::load_from_memory(b).ok()).map(|im| ctx.load_texture(format!("prev{i}"), to_color_image(&im.to_rgba8(), 256), egui::TextureOptions::LINEAR)))
            .collect();
        self.thumbs = mats
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (0..m.maps.len())
                    .map(|s| {
                        let ms = &m.maps[s];
                        if s < 2 || !ms.on {
                            return None;
                        }
                        let img = slot_image(ms, 96)?;
                        let img = image::imageops::resize(&img, 96, 96, image::imageops::FilterType::Triangle);
                        Some(ctx.load_texture(format!("thumb{i}_{s}"), to_color_image(&adjusted(&img, &ms.adj), 96), egui::TextureOptions::LINEAR))
                    })
                    .collect()
            })
            .collect();
        self.color = imgs.iter().enumerate().map(|(i, im)| im.as_ref().map(|im| ctx.load_texture(format!("mat{i}"), to_color_image(im, 512), egui::TextureOptions::LINEAR))).collect();
        self.alpha = imgs
            .iter()
            .enumerate()
            .map(|(i, im)| {
                im.as_ref().map(|im| {
                    let mut a = im.clone();
                    for p in a.pixels_mut() {
                        *p = image::Rgba([p[3], p[3], p[3], 255]);
                    }
                    ctx.load_texture(format!("alpha{i}"), to_color_image(&a, 512), egui::TextureOptions::LINEAR)
                })
            })
            .collect();
        // Atlas: cell 0 is white, then one 128-pixel cell per pictured material.
        const CELL: usize = 128;
        let n = imgs.len() + 1;
        let cols = (n as f32).sqrt().ceil() as usize;
        let rows = n.div_ceil(cols);
        let (w, hgt) = (cols * CELL, rows * CELL);
        let mut atlas = image::RgbaImage::from_pixel(w as u32, hgt as u32, image::Rgba([255, 255, 255, 255]));
        self.cells = vec![None; imgs.len()];
        for (i, im) in imgs.iter().enumerate() {
            let Some(im) = im else { continue };
            let k = i + 1;
            let (cx, cy) = ((k % cols) * CELL, (k / cols) * CELL);
            // Atlas Scale picks the picture's sharpness inside its cell.
            let res = ((CELL as f32 * mats[i].atlas_scale.clamp(0.1, 1.0)) as u32).max(8);
            let small = image::imageops::resize(&image::imageops::resize(im, res, res, image::imageops::FilterType::Triangle), CELL as u32, CELL as u32, image::imageops::FilterType::Nearest);
            image::imageops::replace(&mut atlas, &small, cx as i64, cy as i64);
            let inset = 1.0;
            self.cells[i] = Some([(cx as f32 + inset) / w as f32, (cy as f32 + inset) / hgt as f32, (cx as f32 + CELL as f32 - inset) / w as f32, (cy as f32 + CELL as f32 - inset) / hgt as f32]);
        }
        self.white = [(CELL as f32 * 0.5) / w as f32, (CELL as f32 * 0.5) / hgt as f32];
        self.atlas_img = Some(std::sync::Arc::new(atlas.clone()));
        self.atlas = Some(ctx.load_texture("leaf_atlas", egui::ColorImage::from_rgba_unmultiplied([w, hgt], atlas.as_raw()), egui::TextureOptions::LINEAR));
    }
}

// ---------------------------------------------------------------- small widgets

pub fn checker(painter: &egui::Painter, r: Rect, cell: f32) {
    painter.rect_filled(r, CornerRadius::ZERO, Color32::from_gray(58));
    let (nx, ny) = ((r.width() / cell).ceil() as i32, (r.height() / cell).ceil() as i32);
    for y in 0..ny {
        for x in 0..nx {
            if (x + y) % 2 == 0 {
                let c = Rect::from_min_size(r.min + Vec2::new(x as f32 * cell, y as f32 * cell), Vec2::splat(cell)).intersect(r);
                painter.rect_filled(c, CornerRadius::ZERO, Color32::from_gray(68));
            }
        }
    }
}

fn noise(x: i32, y: i32, salt: u64) -> f32 {
    (hash(salt, ((x as u64) << 32) ^ (y as u64 & 0xffff_ffff)) >> 40) as f32 / (1u64 << 24) as f32
}

fn scale(c: [u8; 3], k: f32) -> Color32 {
    Color32::from_rgb((c[0] as f32 * k).min(255.0) as u8, (c[1] as f32 * k).min(255.0) as u8, (c[2] as f32 * k).min(255.0) as u8)
}

/// A shaded preview sphere, textured with the material's picture when it has one.
fn sphere(painter: &egui::Painter, c: Pos2, rad: f32, m: &Material, tex: Option<&egui::TextureHandle>) {
    let col = m.color();
    let (rings, segs) = (24, 48);
    let mut mesh = egui::Mesh::with_texture(tex.map_or(egui::TextureId::default(), |t| t.id()));
    let light = crate::math::v3(-0.4, 0.55, 0.75).norm();
    for i in 0..=rings {
        let lat = i as f32 / rings as f32 * std::f32::consts::PI;
        for j in 0..=segs {
            let lon = j as f32 / segs as f32 * std::f32::consts::TAU;
            let n = crate::math::v3(lat.sin() * lon.sin(), lat.cos(), lat.sin() * lon.cos());
            let d = n.dot(light).max(0.0);
            let shine = m.maps.get(4).filter(|s| s.on).map_or(0.0, |s| 1.0 - s.v).clamp(0.0, 1.0);
            let spec = d.powf(4.0 + shine * shine * 90.0) * shine * 0.8;
            let k = 0.22 + 0.85 * d;
            let base = if tex.is_some() { [255, 255, 255] } else { col };
            let grain = if tex.is_none() && m.look == 0 { 0.8 + 0.4 * noise(i * 7 + j * 3, j, 3) } else { 1.0 };
            let mut c32 = scale(base, k * grain);
            c32 = Color32::from_rgb(c32.r().saturating_add((spec * 255.0) as u8), c32.g().saturating_add((spec * 255.0) as u8), c32.b().saturating_add((spec * 255.0) as u8));
            let uv = if tex.is_some() { Pos2::new((j as f32 / segs as f32 * 2.0).fract(), i as f32 / rings as f32) } else { egui::epaint::WHITE_UV };
            mesh.vertices.push(egui::epaint::Vertex { pos: c + Vec2::new(n.x, -n.y) * rad, uv, color: c32 });
        }
    }
    let s = segs as u32 + 1;
    for i in 0..rings as u32 {
        for j in 0..segs as u32 {
            let a = i * s + j;
            let (b, cc, d) = (a + 1, a + s, a + s + 1);
            mesh.add_triangle(a, cc, b);
            mesh.add_triangle(b, cc, d);
        }
    }
    // Draw back-to-front by keeping the triangles whose normal faces the viewer.
    let verts = mesh.vertices.clone();
    let mut keep = vec![];
    for t in mesh.indices.chunks(3) {
        let (p0, p1, p2) = (verts[t[0] as usize].pos, verts[t[1] as usize].pos, verts[t[2] as usize].pos);
        if (p1.x - p0.x) * (p2.y - p0.y) - (p1.y - p0.y) * (p2.x - p0.x) < 0.0 {
            keep.extend_from_slice(t);
        }
    }
    mesh.indices = keep;
    painter.add(egui::Shape::mesh(mesh));
}

/// Thumbnail for one texture slot.
fn swatch(painter: &egui::Painter, r: Rect, slot: usize, m: &Material, pics: &Pics, mi: usize) {
    checker(painter, r, 8.0);
    let s = &m.maps[slot];
    if !s.on {
        return;
    }
    let tex = match slot {
        0 => pics.color.get(mi).and_then(|t| t.as_ref()),
        1 => pics.alpha.get(mi).and_then(|t| t.as_ref()),
        _ => pics.thumbs.get(mi).and_then(|t| t.get(slot)).and_then(|t| t.as_ref()),
    };
    if let Some(t) = tex {
        painter.image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        return;
    }
    if MAP_IS_COLOR[slot] {
        painter.rect_filled(r, CornerRadius::same(2), Color32::from_rgb(s.color[0], s.color[1], s.color[2]));
    }
}

fn boxed(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().fill(Color32::from_gray(48)).corner_radius(6).inner_margin(7).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).size(13.0).strong());
        ui.add_space(2.0);
        add(ui);
    });
    ui.add_space(4.0);
}

fn lrow(ui: &mut egui::Ui, w: f32, text: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 22.0), Sense::hover());
        ui.painter().text(Pos2::new(r.right() - 6.0, r.center().y), egui::Align2::RIGHT_CENTER, text, egui::FontId::proportional(12.5), ui.visuals().text_color());
        // Sliders fit what's left of the row, with room for a value like "-1.00".
        ui.spacing_mut().slider_width = (ui.available_width() - 92.0).max(40.0);
        add(ui);
    });
}

fn combo(ui: &mut egui::Ui, id: &str, i: &mut usize, opts: &[&str], w: f32) -> bool {
    let mut ch = false;
    // ComboBox adds its arrow beside `width`; leave room for it.
    egui::ComboBox::from_id_salt(id).width((w - 28.0).max(30.0)).selected_text(opts.get(*i).copied().unwrap_or("")).show_ui(ui, |ui| {
        for (k, o) in opts.iter().enumerate() {
            ch |= ui.selectable_value(i, k, *o).changed();
        }
    });
    ch
}

fn icon_btn(ui: &mut egui::Ui, t: &str, tip: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(t).size(15.0)).min_size(Vec2::new(28.0, 26.0))).on_hover_text(tip).clicked()
}

/// Top row of every editor tab: the item picker and add/copy/delete.
fn picker(ui: &mut egui::Ui, id: &str, names: &[String], empty: &str, sel: &mut usize) -> (bool, bool, bool) {
    let mut out = (false, false, false);
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 140.0).max(60.0);
        let text = names.get(*sel).cloned().unwrap_or_else(|| empty.to_string());
        egui::ComboBox::from_id_salt(id).width(w).selected_text(text).show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                ui.selectable_value(sel, i, n);
            }
        });
        out.1 = icon_btn(ui, "⧉", "Duplicate");
        out.0 = icon_btn(ui, "➕", "Add new");
        out.2 = icon_btn(ui, "🗑", "Delete");
    });
    out
}

/// Returns true when the document changed.
pub fn editors(ui: &mut egui::Ui, doc: &mut Doc, st: &mut EditorState, pics: &Pics) -> bool {
    match st.tab {
        0 => materials(ui, doc, st, pics),
        1 => material_sets(ui, doc, st, pics),
        2 => meshes(ui, doc, st, pics),
        _ => displacements(ui, doc, st),
    }
}

fn pick_image() -> Option<String> {
    rfd::FileDialog::new().add_filter("Pictures", &["png", "jpg", "jpeg", "tga", "bmp"]).pick_file().map(|p| p.display().to_string())
}

fn materials(ui: &mut egui::Ui, doc: &mut Doc, st: &mut EditorState, pics: &Pics) -> bool {
    let mut changed = false;
    let names: Vec<String> = doc.materials.iter().map(|m| m.name.clone()).collect();
    let (add, dup, del) = picker(ui, "matpick", &names, "No materials", &mut st.mat);
    if add {
        doc.materials.push(Material::new(&format!("Material {}", doc.materials.len() + 1), [220, 220, 220], 1));
        st.mat = doc.materials.len() - 1;
        changed = true;
    }
    if dup {
        if let Some(m) = doc.materials.get(st.mat).cloned() {
            doc.materials.push(Material { name: format!("{} copy", m.name), ..m });
            st.mat = doc.materials.len() - 1;
            changed = true;
        }
    }
    if del && st.mat < doc.materials.len() {
        doc.materials.remove(st.mat);
        // Generators that used it fall back to their own tint; later ones shift down.
        for g in &mut doc.gens {
            g.material = match g.material {
                Some(i) if i == st.mat => None,
                Some(i) if i > st.mat => Some(i - 1),
                x => x,
            };
        }
        st.mat = st.mat.saturating_sub(1);
        st.map_edit = None;
        st.area_edit = None;
        changed = true;
    }
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 160.0), Sense::hover());
    checker(ui.painter(), r, 10.0);
    if let Some(m) = doc.materials.get(st.mat) {
        let tex = pics.color.get(st.mat).and_then(|t| t.as_ref());
        match (tex, pics.preview.get(st.mat).and_then(|t| t.as_ref())) {
            (None, Some(pv)) => {
                ui.painter().image(pv.id(), Rect::from_center_size(r.center(), Vec2::splat(r.height() * 0.95)), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            _ => sphere(ui.painter(), r.center(), r.height() * 0.43, m, tex),
        }
    }
    let mat_names: Vec<String> = names.clone();
    let all_meshes: Vec<String> = doc.meshes.iter().map(|m| m.name.clone()).collect();
    let mesh_list: Vec<(usize, String)> = doc.meshes.iter().enumerate().filter(|(_, m)| m.material == Some(st.mat)).map(|(i, m)| (i, m.name.clone())).collect();
    let mi = st.mat;
    let Some(m) = doc.materials.get_mut(st.mat) else {
        ui.label(RichText::new("No materials. Add one with ➕.").weak());
        return changed;
    };
    let mut open_map = None;
    // Cutout row changes, applied after: (old mesh, new mesh or None for a new cutout or
    // usize::MAX to take it off, row).
    let mut links: Vec<(Option<usize>, Option<usize>, usize)> = vec![];
    let mesh_pts: Vec<Vec<[f32; 2]>> = doc.meshes.iter().map(|m| m.lods[0].clone()).collect();
    let split_max = (ui.available_height() - 140.0).max(110.0);
    if st.mat_split <= 0.0 {
        st.mat_split = 330.0;
    }
    st.mat_split = st.mat_split.clamp(100.0, split_max);
    egui::ScrollArea::vertical().id_salt("matscroll").max_height(st.mat_split).auto_shrink([false, false]).show(ui, |ui| {
        // ------------------------------------------------ settings box with its sub-tabs
        egui::Frame::new().fill(Color32::from_gray(48)).corner_radius(6).inner_margin(7).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                for (i, (t, tip)) in [("Settings", "Sides, season, colour variations, back material and atlas"), ("Cutouts/Meshes", "The cutout meshes drawn on this material"), ("UV Areas", "Parts of the picture branches can be mapped to")].iter().enumerate() {
                    ui.selectable_value(&mut st.mat_sub, i, RichText::new(*t).size(12.5)).on_hover_text(*tip);
                }
            });
            ui.separator();
            match st.mat_sub {
                1 => {
                    // The list keeps up with the meshes drawn on this material.
                    for (i, _) in &mesh_list {
                        if !m.cut_rows.contains(&Some(*i)) {
                            m.cut_rows.push(Some(*i));
                        }
                    }
                    let mut sort_now = false;
                    ui.horizontal(|ui| {
                        if ui.button("➕ Add").on_hover_text("Add a cutout row, then pick its mesh (or make a new cutout)").clicked() {
                            m.cut_rows.push(None);
                        }
                        ui.label(RichText::new("Cutouts").weak());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.menu_button("⇅", |ui| {
                                for (k, label) in [(0, "🔤 Name"), (1, "📍 Points"), (2, "⬛ Size")] {
                                    if ui.selectable_label(st.cut_sort.0 == k, label).clicked() {
                                        st.cut_sort.0 = k;
                                        sort_now = true;
                                    }
                                }
                                ui.separator();
                                for (d, label) in [(false, "⬆ Ascending"), (true, "⬇ Descending")] {
                                    if ui.selectable_label(st.cut_sort.1 == d, label).clicked() {
                                        st.cut_sort.1 = d;
                                        sort_now = true;
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Sort the cutouts");
                        });
                    });
                    if sort_now {
                        let (by, desc) = st.cut_sort;
                        let key = |r: &Option<usize>| -> (bool, String, f32) {
                            let Some(i) = *r else { return (true, String::new(), 0.0) };
                            let pts = &mesh_pts[i];
                            let size = if pts.is_empty() { 0.0 } else {
                                let (lo, hi) = pts.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(l, h), q| ([l[0].min(q[0]), l[1].min(q[1])], [h[0].max(q[0]), h[1].max(q[1])]));
                                (hi[0] - lo[0]) * (hi[1] - lo[1])
                            };
                            (false, all_meshes[i].to_lowercase(), if by == 1 { pts.len() as f32 } else { size })
                        };
                        m.cut_rows.sort_by(|a, b| {
                            let (ka, kb) = (key(a), key(b));
                            // Rows still waiting for a mesh stay at the end.
                            let o = if by == 0 { ka.1.cmp(&kb.1) } else { ka.2.total_cmp(&kb.2) };
                            ka.0.cmp(&kb.0).then(if desc { o.reverse() } else { o })
                        });
                        changed = true;
                    }
                    let mut opts: Vec<&str> = vec!["➕ New cutout"];
                    opts.extend(all_meshes.iter().map(String::as_str));
                    let mut rects = vec![];
                    let mut remove = None;
                    for ri in 0..m.cut_rows.len() {
                        let row = m.cut_rows[ri];
                        let r = egui::Frame::new().fill(Color32::from_gray(56)).corner_radius(5).inner_margin(4).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                if grip(ui).drag_started() {
                                    st.row_drag = Some(("cut", ri));
                                }
                                let text = row.and_then(|i| all_meshes.get(i).cloned()).unwrap_or_else(|| "Choose a mesh…".into());
                                let mut sel = usize::MAX;
                                let w = (ui.available_width() - 120.0).max(50.0);
                                egui::ComboBox::from_id_salt(("cutrow", ri)).width(w).selected_text(text).show_ui(ui, |ui| {
                                    for (k, o) in opts.iter().enumerate() {
                                        ui.selectable_value(&mut sel, k, *o);
                                    }
                                });
                                if sel != usize::MAX {
                                    links.push((row, if sel == 0 { None } else { Some(sel - 1) }, ri));
                                }
                                if ui.add_enabled(row.is_some(), egui::Button::new("✏ Edit")).on_hover_text("Open it in the Meshes tab: edit a cutout there, or import a mesh").clicked() {
                                    st.mesh = row.unwrap_or(0);
                                    st.tab = 2;
                                }
                                if ui.small_button("✖").on_hover_text("Take it off this material (the mesh stays in the Meshes tab)").clicked() {
                                    remove = Some(ri);
                                }
                            });
                        });
                        rects.push(r.response.rect);
                    }
                    if let Some((from, to)) = reorder(ui, &mut st.row_drag, "cut", &rects) {
                        let r = m.cut_rows.remove(from);
                        m.cut_rows.insert(to, r);
                        changed = true;
                    }
                    if let Some(ri) = remove {
                        if let Some(Some(i)) = m.cut_rows.get(ri).copied() {
                            links.push((Some(i), Some(usize::MAX), usize::MAX));
                        }
                        m.cut_rows.remove(ri);
                        changed = true;
                    }
                    if m.cut_rows.is_empty() {
                        ui.label(RichText::new("No cutouts yet: ➕ Add one.").weak());
                    }
                }
                2 => {
                    ui.horizontal(|ui| {
                        if ui.button("➕ Add").on_hover_text("Add a UV area and open it in the Area Editor").clicked() {
                            let mut n = 1;
                            while m.uv_areas.iter().any(|a| a.name == format!("Area {n}")) {
                                n += 1;
                            }
                            m.uv_areas.push(UvArea::new(n));
                            if st.area_on_add {
                                st.area_edit = Some(AreaEdit::new(mi, m.uv_areas.len() - 1));
                            }
                            changed = true;
                        }
                        ui.label(RichText::new("Areas").weak());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(&mut st.area_on_add, "Open editor").on_hover_text("Open the Area Editor when an area is added (click ✏ on a row any time)");
                        });
                    });
                    if m.uv_areas.is_empty() {
                        ui.label(RichText::new("No areas yet: branches use the whole picture.").weak());
                    }
                    let mut remove = None;
                    let mut rects = vec![];
                    for (i, a) in m.uv_areas.iter().enumerate() {
                        let on = st.area_edit.as_ref().is_some_and(|e| e.mat == mi && e.area == i);
                        let r = egui::Frame::new().fill(Color32::from_gray(if on { 66 } else { 56 })).corner_radius(5).inner_margin(4).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                if grip(ui).drag_started() {
                                    st.row_drag = Some(("uv", i));
                                }
                                // Buttons first (right to left), then the name takes what's left.
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("🗑").on_hover_text("Delete this area").clicked() {
                                        remove = Some(i);
                                    }
                                    if ui.small_button("✏").on_hover_text("Edit this area").clicked() {
                                        st.area_edit = Some(AreaEdit::new(mi, i));
                                    }
                                    let w = (ui.available_width() - 2.0).max(30.0);
                                    if ui.add_sized([w, 22.0], egui::Button::new(egui::RichText::new(&a.name)).truncate().selected(on)).on_hover_text("Open in the Area Editor").clicked() {
                                        st.area_edit = Some(AreaEdit::new(mi, i));
                                    }
                                });
                            });
                        });
                        rects.push(r.response.rect);
                    }
                    if let Some((from, to)) = reorder(ui, &mut st.row_drag, "uv", &rects) {
                        let a = m.uv_areas.remove(from);
                        m.uv_areas.insert(to, a);
                        // The open editor follows its area.
                        if let Some(e) = st.area_edit.as_mut().filter(|e| e.mat == mi) {
                            e.area = if e.area == from { to } else if from < e.area && e.area <= to { e.area - 1 } else if to <= e.area && e.area < from { e.area + 1 } else { e.area };
                        }
                        changed = true;
                    }
                    if let Some(i) = remove {
                        m.uv_areas.remove(i);
                        st.area_edit = None;
                        changed = true;
                    }
                }
                _ => {
                    lrow(ui, 64.0, "Name", |ui| changed |= ui.add(egui::TextEdit::singleline(&mut m.name).desired_width((ui.available_width() - 8.0).max(40.0))).changed());
                    let half = (ui.available_width() - 8.0) / 2.0;
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(half);
                            boxed(ui, "Main", |ui| {
                                lrow(ui, 72.0, "Two-sided", |ui| changed |= ui.checkbox(&mut m.two_sided, "").on_hover_text("Light both sides of leaves; off leaves the back of each leaf in shade").changed());
                                lrow(ui, 72.0, "Season", |ui| changed |= ui.checkbox(&mut m.season, "").on_hover_text("Leaves with this material change colour with the Season slider").changed());
                            });
                            boxed(ui, "Variations", |ui| {
                                ui.horizontal(|ui| {
                                    for c in &mut m.variations {
                                        changed |= ui.color_edit_button_srgb(c).on_hover_text("A colour leaves may drift toward").changed();
                                    }
                                });
                                lrow(ui, 72.0, "Amount", |ui| changed |= ui.add(egui::Slider::new(&mut m.var_amount, 0.0..=1.0)).on_hover_text("How far each leaf drifts toward one of the three colours (picked at random)").changed());
                            });
                            boxed(ui, "Back", |ui| {
                                lrow(ui, 72.0, "Material", |ui| {
                                    let mut opts = vec!["None"];
                                    opts.extend(mat_names.iter().map(String::as_str));
                                    let mut i = m.back.map_or(0, |x| x + 1);
                                    if combo(ui, "matback", &mut i, &opts, ui.available_width() - 6.0) {
                                        m.back = i.checked_sub(1);
                                        changed = true;
                                    }
                                });
                                lrow(ui, 72.0, "Spacing", |ui| changed |= ui.add(egui::DragValue::new(&mut m.spacing).speed(0.001)).on_hover_text("Gap between the front and back faces in exported files").changed());
                            });
                        });
                        ui.vertical(|ui| {
                            ui.set_width(half);
                            boxed(ui, "Atlas", |ui| {
                                lrow(ui, 72.0, "Scale", |ui| changed |= ui.add(egui::DragValue::new(&mut m.atlas_scale).speed(0.01).range(0.1..=4.0)).on_hover_text("How much room the picture gets in the 3D view's leaf atlas").changed());
                                lrow(ui, 72.0, "Handling", |ui| changed |= combo(ui, "handling", &mut m.handling, &["Default", "Shared", "Unique"], ui.available_width() - 6.0));
                                lrow(ui, 72.0, "Name", |ui| changed |= ui.add(egui::TextEdit::singleline(&mut m.atlas_name).desired_width((ui.available_width() - 8.0).max(40.0))).on_hover_text("Atlas name written to exported files").changed());
                            });
                            boxed(ui, "Other", |ui| {
                                lrow(ui, 96.0, "Max resolution", |ui| changed |= combo(ui, "maxres", &mut m.max_res, &["Default", "256", "512", "1024", "2048", "4096"], ui.available_width() - 6.0));
                                lrow(ui, 96.0, "User data", |ui| changed |= ui.add(egui::TextEdit::singleline(&mut m.user_data).desired_width((ui.available_width() - 8.0).max(40.0))).on_hover_text("Free text kept with the material for your own tools").changed());
                                ui.vertical_centered_justified(|ui| {
                                    if ui.button("Make New Set...").on_hover_text("Start a material set from this material").clicked() {
                                        st.message = Some("new-set".into());
                                    }
                                    if ui.button("Reload Maps").on_hover_text("Read the picture files again").clicked() {
                                        st.message = Some("reload".into());
                                    }
                                });
                            });
                        });
                    });
                }
            }
        });
    });
    // The maps box keeps its place for every sub-tab; drag this line to make it bigger or smaller.
    let (hr, hresp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 12.0), Sense::drag());
    let hot = hresp.hovered() || hresp.dragged();
    ui.painter().line_segment([Pos2::new(hr.left() + 6.0, hr.center().y), Pos2::new(hr.right() - 6.0, hr.center().y)], Stroke::new(if hot { 2.0 } else { 1.0 }, Color32::from_gray(if hot { 150 } else { 80 })));
    for k in -1..=1 {
        ui.painter().circle_filled(Pos2::new(hr.center().x + k as f32 * 7.0, hr.center().y), 1.8, Color32::from_gray(if hot { 210 } else { 140 }));
    }
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if hresp.dragged() {
        st.mat_split = (st.mat_split + hresp.drag_delta().y).clamp(100.0, split_max);
    }
    hresp.on_hover_text("Drag to share the space between the settings and the maps");
    egui::ScrollArea::vertical().id_salt("mapscroll").auto_shrink([false, false]).show(ui, |ui| {
        boxed(ui, "Maps", |ui| {
            ui.label(RichText::new("Click a picture to edit its map (or pick a picture when it has none).").size(11.5).weak());
            let cw = (ui.available_width() - 4.0 * 4.0) / 5.0;
            for row in MAP_SHOWN.chunks(5) {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for &i in row {
                        ui.vertical(|ui| {
                            ui.set_width(cw);
                            egui::Frame::new().fill(Color32::from_gray(52)).corner_radius(4).inner_margin(4).show(ui, |ui| {
                                ui.set_width(cw - 8.0);
                                ui.label(RichText::new(map_label(i)).size(11.5));
                                let slot = &mut m.maps[i];
                                if MAP_IS_COLOR[i] {
                                    ui.horizontal(|ui| {
                                        changed |= ui.color_edit_button_srgb(&mut slot.color).on_hover_text("Colour used when there's no picture (and to tint it)").changed();
                                    });
                                } else {
                                    changed |= ui.add_sized([cw - 8.0, 20.0], egui::DragValue::new(&mut slot.v).speed(0.01).range(0.0..=1.0)).on_hover_text("Value used when there's no picture (and to scale it)").changed();
                                }
                                ui.horizontal(|ui| {
                                    changed |= ui.checkbox(&mut slot.on, "").on_hover_text("Use this map").changed();
                                    if slot.file.is_some() && ui.small_button("✖").on_hover_text("Remove the picture").clicked() {
                                        slot.file = None;
                                        changed = true;
                                    }
                                });
                                let (r, resp) = ui.allocate_exact_size(Vec2::new(cw - 8.0, cw - 12.0), Sense::click());
                                let mm = m.clone();
                                swatch(ui.painter(), r, i, &mm, pics, mi);
                                let tip = m.maps[i].file.clone().map_or_else(|| "Click to pick a picture".into(), |f| format!("{f}\nClick to edit the map"));
                                if resp.on_hover_text(tip).clicked() {
                                    open_map = Some(i);
                                }
                            });
                        });
                    }
                });
            }
        });
    });
    for (old, new, row) in links {
        let rows = doc.materials.get(mi).map(|m| m.cut_rows.clone()).unwrap_or_default();
        // An old mesh no other row uses comes off the material.
        if let Some(o) = old.filter(|o| rows.iter().filter(|r| **r == Some(*o)).count() <= 1 || new == Some(usize::MAX)) {
            if let Some(mm) = doc.meshes.get_mut(o) {
                mm.material = None;
            }
        }
        match new {
            Some(n) if n < doc.meshes.len() => {
                doc.meshes[n].material = Some(mi);
                if let Some(r) = doc.materials.get_mut(mi).and_then(|m| m.cut_rows.get_mut(row)) {
                    *r = Some(n);
                }
            }
            Some(_) => {}
            None => {
                // A new cutout joins the list by itself; this row goes.
                if let Some(m) = doc.materials.get_mut(mi).filter(|m| row < m.cut_rows.len()) {
                    m.cut_rows.remove(row);
                }
                st.message = Some("new-cutout".into());
            }
        }
        changed = true;
    }
    let Some(m) = doc.materials.get_mut(mi) else { return changed };
    // A picture slot opens the Edit Map dialog; with no picture yet, the file picker comes first.
    if let Some(i) = open_map {
        st.map_edit = Some(MapEdit::new(mi, i));
        if m.maps[i].gen.source == 0 && m.maps[i].file.is_none() && !st.no_ask_pick {
            if let Some(f) = pick_image() {
                m.maps[i].file = Some(f);
                m.maps[i].on = true;
                changed = true;
            }
        }
    }
    changed
}

/// The Texture Edit and UV Area dialogs, shown whichever tab is open (they close by hand).
pub fn material_windows(ctx: &egui::Context, st: &mut EditorState, doc: &mut Doc, pics: &Pics) -> bool {
    map_editor(ctx, st, doc) | area_editor(ctx, st, doc, pics)
}

/// A drag handle for reordering a list row.
fn grip(ui: &mut egui::Ui) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::new(14.0, 20.0), Sense::drag());
    let col = if resp.hovered() || resp.dragged() { Color32::from_gray(225) } else { Color32::from_gray(140) };
    for k in -1..=1 {
        let y = r.center().y + k as f32 * 4.0;
        ui.painter().line_segment([Pos2::new(r.left() + 2.0, y), Pos2::new(r.right() - 2.0, y)], Stroke::new(1.5, col));
    }
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
    }
    resp.on_hover_text("Drag to reorder")
}

/// A row drag in `list`: shows where it would land, and gives (from, to) when let go.
fn reorder(ui: &mut egui::Ui, drag: &mut Option<(&'static str, usize)>, list: &'static str, rects: &[Rect]) -> Option<(usize, usize)> {
    let from = match *drag {
        Some((l, f)) if l == list && f < rects.len() => f,
        _ => return None,
    };
    let y = ui.input(|i| i.pointer.interact_pos().map(|p| p.y));
    let to = y.map(|y| rects.iter().position(|r| y < r.center().y).unwrap_or(rects.len()));
    if let (Some(to), Some(first)) = (to, rects.first()) {
        let line_y = if to < rects.len() { rects[to].top() - 2.0 } else { rects[rects.len() - 1].bottom() + 2.0 };
        ui.painter().line_segment([Pos2::new(first.left(), line_y), Pos2::new(first.right(), line_y)], Stroke::new(2.0, Color32::from_rgb(0x47, 0x72, 0xb3)));
    }
    if ui.input(|i| i.pointer.any_down()) {
        return None;
    }
    *drag = None;
    let to = to?;
    let to = if to > from { to - 1 } else { to };
    (to != from).then_some((from, to))
}

/// The Edit Map dialog's state.
pub struct MapEdit {
    mat: usize,
    slot: usize,
    src: Option<image::RgbaImage>,
    src_sig: String,
    tex: Option<egui::TextureHandle>,
    sig: String,
    zoom: f32,
    pan: Vec2,
    /// The selected colour-ramp stop.
    stop: usize,
    /// Dragging a gradient end on the preview (0 start, 1 end).
    handle: Option<usize>,
}

impl MapEdit {
    pub fn new(mat: usize, slot: usize) -> MapEdit {
        MapEdit { mat, slot, src: None, src_sig: "-".into(), tex: None, sig: "-".into(), zoom: 1.0, pan: Vec2::ZERO, stop: 0, handle: None }
    }
}

/// A map slot's picture before its adjustments: generated, or read from its file.
pub fn slot_image(s: &MapSlot, size: u32) -> Option<image::RgbaImage> {
    match s.gen.source {
        0 => image::open(s.file.as_ref()?).ok().map(|i| i.to_rgba8()),
        1 => None,
        _ => Some(s.gen.image(size)),
    }
}

/// A map's picture with its Edit Map adjustments applied.
pub fn adjusted(img: &image::RgbaImage, a: &MapAdjust) -> image::RgbaImage {
    if *a == MapAdjust::default() {
        return img.clone();
    }
    let luma = |p: &image::Rgba<u8>| (p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114) / 255.0;
    let (lo, hi) = if a.normalize { img.pixels().fold((1.0f32, 0.0f32), |(l, h), p| (l.min(luma(p)), h.max(luma(p)))) } else { (0.0, 1.0) };
    let mut out = img.clone();
    for p in out.pixels_mut() {
        let q = a.pixel([p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0, p[3] as f32 / 255.0], lo, hi);
        *p = image::Rgba(q.map(|x| (x * 255.0).round() as u8));
    }
    out
}

/// A slider with a reset button, for the Edit Map dialog.
fn adj_slider(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>, reset: f32, tip: &str) -> bool {
    let mut ch = false;
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(78.0, 22.0), Sense::hover());
        ui.painter().text(Pos2::new(r.right() - 6.0, r.center().y), egui::Align2::RIGHT_CENTER, label, egui::FontId::proportional(12.5), ui.visuals().text_color());
        ui.spacing_mut().slider_width = 120.0;
        ch |= ui.add(egui::Slider::new(v, range)).on_hover_text(tip).changed();
        if ui.add_enabled(*v != reset, egui::Button::new("↺").small()).on_hover_text("Reset").clicked() {
            *v = reset;
            ch = true;
        }
    });
    ch
}

/// A colour ramp editor, like a gradient map: click the bar to add a colour, drag the markers
/// under it to move them, pick a marker to set its colour or remove it.
fn ramp_ui(ui: &mut egui::Ui, ramp: &mut Ramp, sel: &mut usize, id: &str, width: f32) -> bool {
    let mut ch = false;
    let (bar, bresp) = ui.allocate_exact_size(Vec2::new(width, 22.0), Sense::click());
    let p = ui.painter();
    checker(p, bar, 6.0);
    let mut m = egui::Mesh::default();
    let x_at = |t: f32| bar.left() + t.clamp(0.0, 1.0) * bar.width();
    let mut cuts: Vec<f32> = ramp.stops.iter().map(|s| s.0).collect();
    cuts.insert(0, 0.0);
    cuts.push(1.0);
    for w in cuts.windows(2) {
        let (a, b) = (ramp.eval(w[0]), ramp.eval(w[1]));
        let base = m.vertices.len() as u32;
        let (ca, cb) = (Color32::from_rgb(a[0], a[1], a[2]), Color32::from_rgb(b[0], b[1], b[2]));
        m.colored_vertex(Pos2::new(x_at(w[0]), bar.top()), ca);
        m.colored_vertex(Pos2::new(x_at(w[1]), bar.top()), cb);
        m.colored_vertex(Pos2::new(x_at(w[1]), bar.bottom()), cb);
        m.colored_vertex(Pos2::new(x_at(w[0]), bar.bottom()), ca);
        m.add_triangle(base, base + 1, base + 2);
        m.add_triangle(base, base + 2, base + 3);
    }
    p.add(egui::Shape::mesh(m));
    p.rect_stroke(bar, CornerRadius::same(2), Stroke::new(1.0, Color32::from_gray(80)), egui::StrokeKind::Outside);
    if let Some(at) = bresp.on_hover_text("Click to add a colour here").clicked().then(|| ui.input(|i| i.pointer.interact_pos())).flatten() {
        let t = ((at.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
        ramp.stops.push((t, ramp.eval(t)));
        ramp.sort();
        *sel = ramp.stops.iter().position(|s| s.0 == t).unwrap_or(0);
        ch = true;
    }
    // Markers under the bar.
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, 14.0), Sense::hover());
    let mut moved = None;
    for (k, stop) in ramp.stops.iter().enumerate() {
        let x = x_at(stop.0);
        let r = Rect::from_center_size(Pos2::new(x, row.center().y), Vec2::new(12.0, 14.0));
        let resp = ui.interact(r, ui.make_persistent_id((id, "stop", k)), Sense::click_and_drag());
        let on = *sel == k;
        let tri = vec![Pos2::new(x, row.top()), Pos2::new(x + 6.0, row.bottom()), Pos2::new(x - 6.0, row.bottom())];
        ui.painter().add(egui::Shape::convex_polygon(tri, Color32::from_rgb(stop.1[0], stop.1[1], stop.1[2]), Stroke::new(if on { 2.0 } else { 1.0 }, if on { Color32::WHITE } else { Color32::from_gray(30) })));
        if resp.clicked() || resp.drag_started() {
            *sel = k;
        }
        if resp.dragged() {
            moved = Some((k, ((x + resp.drag_delta().x - bar.left()) / bar.width()).clamp(0.0, 1.0)));
        }
        resp.on_hover_text("Drag to move; click to pick");
    }
    if let Some((k, t)) = moved {
        let c = ramp.stops[k].1;
        ramp.stops[k].0 = t;
        ramp.sort();
        *sel = ramp.stops.iter().position(|s| s.0 == t && s.1 == c).unwrap_or(k);
        ch = true;
    }
    *sel = (*sel).min(ramp.stops.len().saturating_sub(1));
    ui.horizontal(|ui| {
        if let Some(stop) = ramp.stops.get_mut(*sel) {
            ui.label("Colour");
            ch |= ui.color_edit_button_srgb(&mut stop.1).on_hover_text("The picked marker's colour").changed();
            ch |= ui.add(egui::DragValue::new(&mut stop.0).speed(0.005).range(0.0..=1.0).max_decimals(3)).on_hover_text("The picked marker's place on the bar").changed();
        }
        if ui.add_enabled(ramp.stops.len() > 2, egui::Button::new("🗑").small()).on_hover_text("Remove the picked marker").clicked() {
            ramp.stops.remove(*sel);
            *sel = sel.saturating_sub(1);
            ch = true;
        }
        if ui.small_button("⇄").on_hover_text("Reverse the colours").clicked() {
            for s in &mut ramp.stops {
                s.0 = 1.0 - s.0;
            }
            ramp.sort();
            ch = true;
        }
    });
    if ch {
        ramp.sort();
    }
    ch
}

/// The Edit Map dialog: the slot's picture (a file, or a generated gradient or noise), its
/// adjustments and a live preview you can zoom and pan.
fn map_editor(ctx: &egui::Context, st: &mut EditorState, doc: &mut Doc) -> bool {
    let Some(me) = st.map_edit.as_mut() else { return false };
    let Some(m) = doc.materials.get_mut(me.mat) else {
        st.map_edit = None;
        return false;
    };
    let slot = me.slot;
    let mut changed = false;
    // Read or make the picture again when its source changes.
    let src_sig = serde_json::to_string(&(&m.maps[slot].file, &m.maps[slot].gen)).unwrap_or_default();
    if src_sig != me.src_sig {
        me.src_sig = src_sig;
        me.src = slot_image(&m.maps[slot], 256).map(|i| if i.width() > 512 || i.height() > 512 { image::imageops::resize(&i, 512, 512, image::imageops::FilterType::Triangle) } else { i });
        me.sig = "-".into();
    }
    let sig = serde_json::to_string(&m.maps[slot].adj).unwrap_or_default();
    if sig != me.sig {
        me.sig = sig;
        me.tex = me.src.as_ref().map(|img| ctx.load_texture("map_edit_preview", to_color_image(&adjusted(img, &m.maps[slot].adj), 512), egui::TextureOptions::LINEAR));
    }
    let mut open = true;
    let screen = ctx.content_rect();
    egui::Window::new(format!("Texture Edit - {}", map_label(slot)))
        .id(egui::Id::unique("map_editor"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([800.0, 600.0])
        .default_pos(screen.center() - Vec2::new(400.0, 300.0))
        .show(ctx, |ui| {
            let ms = &mut m.maps[slot];
            // ------------------------------------------------ source bar
            egui::Frame::new().fill(Color32::from_gray(44)).corner_radius(6).inner_margin(6).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let before = ms.gen.source;
                    if combo(ui, "map_src", &mut ms.gen.source, &SOURCES, 120.0) {
                        if ms.gen.source >= 2 && before < 2 {
                            ms.gen.ramp = Ramp::default();
                        }
                        changed = true;
                    }
                    match ms.gen.source {
                        0 => {
                            let mut path = ms.file.clone().unwrap_or_default();
                            ui.add(egui::TextEdit::singleline(&mut path).desired_width(300.0).interactive(false)).on_hover_text("The picture this map reads");
                            if ui.button("…").on_hover_text("Choose a picture").clicked() {
                                if let Some(f) = pick_image() {
                                    ms.file = Some(f);
                                    ms.on = true;
                                    changed = true;
                                }
                            }
                            if ui.button("⟳").on_hover_text("Read the picture file again").clicked() {
                                me.src_sig = "-".into();
                                st.message = Some("reload".into());
                            }
                            if ui.button("🗑").on_hover_text("Remove the picture").clicked() {
                                ms.file = None;
                                changed = true;
                            }
                            let mut ask = !st.no_ask_pick;
                            if ui.checkbox(&mut ask, "Ask").on_hover_text("Open the file picker when an empty map slot is clicked").changed() {
                                st.no_ask_pick = !ask;
                            }
                        }
                        1 => {
                            if MAP_IS_COLOR[slot] {
                                changed |= ui.color_edit_button_srgb(&mut ms.color).on_hover_text("The colour this map uses").changed();
                            } else {
                                changed |= ui.add(egui::Slider::new(&mut ms.v, 0.0..=1.0)).on_hover_text("The value this map uses").changed();
                            }
                            ui.label(RichText::new("No picture: this colour is used.").weak());
                        }
                        _ => {
                            ui.menu_button("✨ Presets", |ui| {
                                for (name, g) in texgen::presets() {
                                    if (g.source == ms.gen.source || ui.input(|i| i.modifiers.shift)) && ui.button(name).clicked() {
                                        let p = (ms.gen.p0, ms.gen.p1);
                                        ms.gen = g;
                                        if ms.gen.source == 2 && p != (MapGen::default().p0, MapGen::default().p1) {
                                            (ms.gen.p0, ms.gen.p1) = p;
                                        }
                                        changed = true;
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Ready-made settings for this kind of picture");
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("⊡ Fit").on_hover_text("Fit the picture in the preview").clicked() {
                            me.zoom = 1.0;
                            me.pan = Vec2::ZERO;
                        }
                    });
                });
            });
            ui.add_space(4.0);
            let body_h = (ui.available_height() - 4.0).max(200.0);
            ui.horizontal_top(|ui| {
                // ------------------------------------------------ settings column
                ui.vertical(|ui| {
                    ui.set_width(300.0);
                    egui::ScrollArea::vertical().id_salt("map_settings").max_height(body_h).auto_shrink([false, false]).show(ui, |ui| {
                        ui.set_width(290.0);
                        if ms.gen.source >= 2 {
                            egui::Frame::new().fill(Color32::from_gray(44)).corner_radius(6).inner_margin(8).show(ui, |ui| {
                                ui.set_width(272.0);
                                let g = &mut ms.gen;
                                if g.source == 2 {
                                    ui.label(RichText::new("Gradient").strong());
                                    lrow(ui, 70.0, "Type", |ui| changed |= combo(ui, "grad_kind", &mut g.gradient, &GRADIENTS, 170.0));
                                    for (k, name) in ["Start", "End"].iter().enumerate() {
                                        let pt = if k == 0 { &mut g.p0 } else { &mut g.p1 };
                                        lrow(ui, 70.0, name, |ui| {
                                            for v in pt.iter_mut() {
                                                changed |= ui.add(egui::DragValue::new(v).speed(0.005).range(-1.0..=2.0).max_decimals(3)).on_hover_text("Drag the dot on the preview, or type a place (0..1)").changed();
                                            }
                                        });
                                    }
                                } else {
                                    ui.label(RichText::new("Noise").strong());
                                    lrow(ui, 70.0, "Type", |ui| changed |= combo(ui, "noise_kind", &mut g.noise, &NOISES, 170.0));
                                    lrow(ui, 70.0, "Scale", |ui| changed |= ui.add(egui::Slider::new(&mut g.scale, 0.5..=40.0).logarithmic(true)).on_hover_text("How many cells or blobs across the picture").changed());
                                    lrow(ui, 70.0, "Detail", |ui| changed |= ui.add(egui::Slider::new(&mut g.detail, 1..=8)).on_hover_text("Layers of finer noise (Clouds, Ridged, Marble, Wood)").changed());
                                    lrow(ui, 70.0, "Pan", |ui| {
                                        for v in g.offset.iter_mut() {
                                            changed |= ui.add(egui::DragValue::new(v).speed(0.005).max_decimals(3)).on_hover_text("Slide the pattern (or drag the dot in the middle of the preview)").changed();
                                        }
                                    });
                                    lrow(ui, 70.0, "Seed", |ui| {
                                        changed |= ui.add(egui::DragValue::new(&mut g.seed)).on_hover_text("A different random pattern").changed();
                                        if ui.small_button("🎲").on_hover_text("New random pattern").clicked() {
                                            g.seed = g.seed.wrapping_mul(2_654_435_761).wrapping_add(97) % 100_000;
                                            changed = true;
                                        }
                                    });
                                }
                                ui.add_space(4.0);
                                ui.label("Colours");
                                changed |= ramp_ui(ui, &mut g.ramp, &mut me.stop, "map_ramp", 268.0);
                            });
                            ui.add_space(4.0);
                        }
                        egui::Frame::new().fill(Color32::from_gray(44)).corner_radius(6).inner_margin(8).show(ui, |ui| {
                            ui.set_width(272.0);
                            ui.label(RichText::new("Adjustments").strong());
                            let a = &mut ms.adj;
                            ui.horizontal(|ui| {
                                ui.label("Channel");
                                changed |= combo(ui, "map_channel", &mut a.channel, &["RGB", "Red", "Green", "Blue", "Alpha"], 110.0);
                                ui.menu_button("📋", |ui| {
                                    if ui.button("Copy adjustments").clicked() {
                                        st.adj_clip = Some(a.clone());
                                        ui.close();
                                    }
                                    if ui.add_enabled(st.adj_clip.is_some(), egui::Button::new("Paste adjustments")).clicked() {
                                        if let Some(c) = st.adj_clip.clone() {
                                            *a = c;
                                            changed = true;
                                        }
                                        ui.close();
                                    }
                                    if ui.button("Reset all").clicked() {
                                        *a = MapAdjust::default();
                                        changed = true;
                                        ui.close();
                                    }
                                })
                                .response
                                .on_hover_text("Copy, paste or reset these adjustments");
                            });
                            changed |= ui.checkbox(&mut a.srgb_to_linear, "sRGB to Linear").on_hover_text("Darken mid-tones the way a linear-light picture would look").changed();
                            changed |= ui.checkbox(&mut a.normalize, "Normalize").on_hover_text("Stretch the picture so its darkest part is black and brightest white").changed();
                            changed |= ui.checkbox(&mut a.invert, "Invert").on_hover_text("Turn dark into light").changed();
                            ui.horizontal(|ui| {
                                ui.label("Invert RGB");
                                for (k, n) in ["R", "G", "B"].iter().enumerate() {
                                    changed |= ui.checkbox(&mut a.invert_rgb[k], *n).on_hover_text(format!("Invert the {n} channel")).changed();
                                }
                            });
                            changed |= adj_slider(ui, "Brightness", &mut a.brightness, -1.0..=1.0, 0.0, "Add to every channel");
                            changed |= adj_slider(ui, "Contrast", &mut a.contrast, -1.0..=1.0, 0.0, "Spread values away from (or toward) mid-grey");
                            changed |= adj_slider(ui, "Saturation", &mut a.saturation, -1.0..=1.0, 0.0, "More or less colour (−1 is grey)");
                            changed |= adj_slider(ui, "Red", &mut a.red, -1.0..=1.0, 0.0, "Add to the red channel");
                            changed |= adj_slider(ui, "Green", &mut a.green, -1.0..=1.0, 0.0, "Add to the green channel");
                            changed |= adj_slider(ui, "Blue", &mut a.blue, -1.0..=1.0, 0.0, "Add to the blue channel");
                            changed |= adj_slider(ui, "Minimum", &mut a.min, 0.0..=1.0, 0.0, "This value and below become black");
                            changed |= adj_slider(ui, "Maximum", &mut a.max, 0.0..=1.0, 1.0, "This value and above become white");
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label("Recolor").on_hover_text("Map the picture's brightness onto a gradient from the first colour to the second");
                                changed |= ui.color_edit_button_srgb(&mut a.recolor[0]).on_hover_text("Colour for the darkest parts").changed();
                                changed |= ui.color_edit_button_srgb(&mut a.recolor[1]).on_hover_text("Colour for the brightest parts").changed();
                                let (r, _) = ui.allocate_exact_size(Vec2::new(80.0, 16.0), Sense::hover());
                                let mut g = egui::Mesh::default();
                                let (c0, c1) = (a.recolor[0], a.recolor[1]);
                                g.colored_vertex(r.left_top(), Color32::from_rgb(c0[0], c0[1], c0[2]));
                                g.colored_vertex(r.right_top(), Color32::from_rgb(c1[0], c1[1], c1[2]));
                                g.colored_vertex(r.right_bottom(), Color32::from_rgb(c1[0], c1[1], c1[2]));
                                g.colored_vertex(r.left_bottom(), Color32::from_rgb(c0[0], c0[1], c0[2]));
                                g.add_triangle(0, 1, 2);
                                g.add_triangle(0, 2, 3);
                                ui.painter().add(egui::Shape::mesh(g));
                                if ui.add_enabled(a.recolor != [[0; 3], [255; 3]], egui::Button::new("↺").small()).on_hover_text("No recolor").clicked() {
                                    a.recolor = [[0; 3], [255; 3]];
                                    changed = true;
                                }
                            });
                        });
                    });
                });
                // ------------------------------------------------ preview (fixed to the space left, so the window keeps its size)
                let side = (ui.available_width() - 4.0).min(body_h).max(140.0);
                let (r, resp) = ui.allocate_exact_size(Vec2::splat(side), Sense::click_and_drag());
                let p = ui.painter_at(r);
                p.rect_filled(r, CornerRadius::same(6), Color32::from_gray(30));
                if resp.hovered() {
                    let z = ui.input(|i| i.smooth_scroll_delta.y);
                    if z != 0.0 {
                        me.zoom = (me.zoom * (z * 0.002).exp()).clamp(0.25, 16.0);
                    }
                }
                let img = Rect::from_center_size(r.center() + me.pan, Vec2::splat((side - 16.0) * me.zoom));
                checker(&p, img.intersect(r), 10.0);
                match &me.tex {
                    Some(t) => {
                        p.image(t.id(), img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        p.text(r.center(), egui::Align2::CENTER_CENTER, "No picture", egui::FontId::proportional(13.0), Color32::from_gray(200));
                    }
                }
                let to = |q: [f32; 2]| Pos2::new(img.left() + q[0] * img.width(), img.top() + q[1] * img.height());
                let from = |s: Pos2| [(s.x - img.left()) / img.width(), (s.y - img.top()) / img.height()];
                let gradient = ms.gen.source == 2;
                let noise = ms.gen.source == 3;
                let pan_dot = Pos2::new(img.center().x + ms.gen.offset[0] * img.width(), img.center().y + ms.gen.offset[1] * img.height());
                if noise {
                    p.circle_filled(pan_dot, 7.0, Color32::from_rgb(250, 160, 50));
                    p.circle_stroke(pan_dot, 7.0, Stroke::new(1.5, Color32::WHITE));
                }
                if gradient {
                    let (a, b) = (to(ms.gen.p0), to(ms.gen.p1));
                    p.line_segment([a, b], Stroke::new(1.5, Color32::WHITE));
                    for (k, s) in [a, b].iter().enumerate() {
                        p.circle_filled(*s, 7.0, if k == 0 { Color32::from_rgb(60, 60, 60) } else { Color32::WHITE });
                        p.circle_stroke(*s, 7.0, Stroke::new(1.5, Color32::from_rgb(250, 160, 50)));
                    }
                }
                if resp.drag_started() {
                    let o = ui.input(|i| i.pointer.press_origin()).unwrap_or(r.center());
                    me.handle = if gradient {
                        [to(ms.gen.p0), to(ms.gen.p1)].iter().position(|s| s.distance(o) < 11.0)
                    } else if noise && pan_dot.distance(o) < 11.0 {
                        Some(2)
                    } else {
                        None
                    };
                }
                if resp.dragged() {
                    match (me.handle, resp.interact_pointer_pos()) {
                        (Some(2), _) => {
                            let d = resp.drag_delta();
                            ms.gen.offset[0] += d.x / img.width();
                            ms.gen.offset[1] += d.y / img.height();
                            changed = true;
                        }
                        (Some(k), Some(at)) => {
                            let q = from(at);
                            if k == 0 { ms.gen.p0 = q } else { ms.gen.p1 = q }
                            changed = true;
                        }
                        _ => me.pan += resp.drag_delta(),
                    }
                }
                if resp.drag_stopped() {
                    me.handle = None;
                }
                if resp.double_clicked() {
                    me.zoom = 1.0;
                    me.pan = Vec2::ZERO;
                }
                resp.on_hover_text(if gradient { "Drag the dots to move the gradient's start and end · wheel: zoom · drag: pan · double-click: fit" } else { "Wheel: zoom · drag: pan · double-click: fit" });
            });
        });
    if !open {
        st.map_edit = None;
    }
    changed
}

/// The UV Area editor's state.
pub struct AreaEdit {
    mat: usize,
    area: usize,
    show_opacity: bool,
    show_all: bool,
    clamp: bool,
    /// Corner being dragged (0..4), or 4 for the whole area.
    drag: Option<usize>,
    last: [f32; 2],
}

impl AreaEdit {
    pub fn new(mat: usize, area: usize) -> AreaEdit {
        AreaEdit { mat, area, show_opacity: false, show_all: false, clamp: true, drag: None, last: [0.0; 2] }
    }
}

/// Move corner `k` of a rectangular area to `q`: the opposite corner stays, the two others
/// follow along their shared edges.
fn drag_rect_corner(pts: &mut [[f32; 2]; 4], k: usize, q: [f32; 2]) {
    let o = pts[(k + 2) % 4];
    let old = *pts;
    pts[k] = q;
    for j in [(k + 1) % 4, (k + 3) % 4] {
        let shares_x = (old[j][0] - old[k][0]).abs() < (old[j][1] - old[k][1]).abs();
        pts[j] = if shares_x { [q[0], o[1]] } else { [o[0], q[1]] };
    }
}

/// The Edit UV Area dialog: four corners (0 is the pivot) over the material's picture.
fn area_editor(ctx: &egui::Context, st: &mut EditorState, doc: &mut Doc, pics: &Pics) -> bool {
    let Some(ae) = st.area_edit.as_mut() else { return false };
    let mi = ae.mat;
    let all: Vec<[[f32; 2]; 4]> = doc.materials.get(mi).map(|m| m.uv_areas.iter().map(|a| a.pts).collect()).unwrap_or_default();
    let Some(area) = doc.materials.get_mut(mi).and_then(|m| m.uv_areas.get_mut(ae.area)) else {
        st.area_edit = None;
        return false;
    };
    let mut changed = false;
    let mut open = true;
    let screen = ctx.content_rect();
    egui::Window::new("Edit UV Area")
        .id(egui::Id::unique("area_editor"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([640.0, 520.0])
        .default_pos(screen.center() - Vec2::new(320.0, 260.0))
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                // ------------------------------------------------ settings (left)
                ui.vertical(|ui| {
                    ui.set_width(236.0);
                    let section = |ui: &mut egui::Ui, title: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                        egui::Frame::new().fill(Color32::from_gray(44)).corner_radius(6).inner_margin(6).show(ui, |ui| {
                            ui.set_width(224.0);
                            ui.label(RichText::new(title).strong());
                            add(ui);
                        });
                        ui.add_space(4.0);
                    };
                    section(ui, "Area", &mut |ui| {
                        ui.horizontal(|ui| {
                            ui.add_sized([70.0, 20.0], egui::Label::new("Name"));
                            changed |= ui.add_sized([140.0, 22.0], egui::TextEdit::singleline(&mut area.name)).changed();
                        });
                        ui.horizontal(|ui| {
                            ui.add_sized([70.0, 20.0], egui::Label::new("Edit mode"));
                            let before = area.mode;
                            if combo(ui, "area_mode", &mut area.mode, &["Vertical", "Horizontal", "Free"], 140.0) {
                                // Vertical and horizontal areas differ by which corner is the pivot.
                                if (before == 0 && area.mode == 1) || (before == 1 && area.mode == 0) {
                                    area.pts.rotate_left(if area.mode == 1 { 1 } else { 3 });
                                }
                                changed = true;
                            }
                        })
                        .response
                        .on_hover_text("Vertical and Horizontal keep a rectangle (they differ in which corner is the pivot); Free moves corners on their own");
                    });
                    // Corners in a 2×2 grid placed like they are on the picture: the higher two on top.
                    section(ui, "Corners", &mut |ui| {
                        let mut idx = [0usize, 1, 2, 3];
                        idx.sort_by(|a, b| area.pts[*b][1].total_cmp(&area.pts[*a][1]));
                        let mut rows = [[idx[0], idx[1]], [idx[2], idx[3]]];
                        for r in &mut rows {
                            r.sort_by(|a, b| area.pts[*a][0].total_cmp(&area.pts[*b][0]));
                        }
                        for row in rows {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                for k in row {
                                    egui::Frame::new().fill(Color32::from_gray(52)).corner_radius(5).inner_margin(5).show(ui, |ui| ui.vertical(|ui| {
                                        ui.set_width(98.0);
                                        ui.label(RichText::new(if k == 0 { "0 Pivot".to_string() } else { k.to_string() }).color(if k == 0 { Color32::from_rgb(250, 160, 50) } else { Color32::from_gray(210) }))
                                            .on_hover_text(if k == 0 { "The corner the area is measured from" } else { "A corner of the area" });
                                        let mut q = area.pts[k];
                                        let mut moved = false;
                                        for (c, n) in ["X", "Y"].iter().enumerate() {
                                            ui.horizontal(|ui| {
                                                ui.label(*n);
                                                moved |= ui.add_sized([74.0, 20.0], egui::DragValue::new(&mut q[c]).speed(0.005).max_decimals(3)).changed();
                                            });
                                        }
                                        if moved {
                                            if ae.clamp {
                                                q = q.map(|x| x.clamp(0.0, 1.0));
                                            }
                                            if area.mode == 2 { area.pts[k] = q } else { drag_rect_corner(&mut area.pts, k, q) }
                                            changed = true;
                                        }
                                    }));
                                }
                            });
                        }
                    });
                    section(ui, "View", &mut |ui| {
                        ui.checkbox(&mut ae.clamp, "Clamp 0-1").on_hover_text("Keep corners inside the picture");
                        ui.radio_value(&mut ae.show_opacity, false, "Show color").on_hover_text("Show the Color map");
                        ui.radio_value(&mut ae.show_opacity, true, "Show opacity").on_hover_text("Show the Opacity map");
                        ui.checkbox(&mut ae.show_all, "Show all areas").on_hover_text("Show this material's other areas too");
                    });
                });
                // ------------------------------------------------ picture (right)
                ui.vertical(|ui| {
                    let side = ui.available_width().min(ui.available_height() - 6.0).max(160.0);
                    let (r, resp) = ui.allocate_exact_size(Vec2::splat(side), Sense::click_and_drag());
                    let p = ui.painter_at(r);
                    // A margin so the corner handles at the picture's edges show whole.
                    let (r, side) = (r.shrink(10.0), side - 20.0);
                    checker(&p, r, 10.0);
                    let tex = if ae.show_opacity { pics.alpha.get(mi) } else { pics.color.get(mi) }.and_then(|t| t.as_ref());
                    if let Some(t) = tex {
                        p.image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    let to = |q: [f32; 2]| Pos2::new(r.left() + q[0] * side, r.bottom() - q[1] * side);
                    let from = |s: Pos2| [(s.x - r.left()) / side, (r.bottom() - s.y) / side];
                    if ae.show_all {
                        for (i, pts) in all.iter().enumerate().filter(|(i, _)| *i != ae.area) {
                            let poly: Vec<Pos2> = pts.iter().map(|&q| to(q)).collect();
                            p.add(egui::Shape::closed_line(poly, Stroke::new(1.2, Color32::from_rgba_unmultiplied(255, 220, 120, 160))));
                            p.text(to(pts[0]) + Vec2::new(4.0, -4.0), egui::Align2::LEFT_BOTTOM, format!("{}", i + 1), egui::FontId::proportional(10.5), Color32::from_rgb(255, 220, 120));
                        }
                    }
                    let poly: Vec<Pos2> = area.pts.iter().map(|&q| to(q)).collect();
                    p.add(egui::Shape::convex_polygon(poly.clone(), Color32::from_rgba_unmultiplied(120, 250, 255, 60), Stroke::new(1.6, Color32::from_rgb(90, 230, 245))));
                    for (k, &s) in poly.iter().enumerate() {
                        p.circle_filled(s, 8.0, Color32::from_rgb(220, 40, 40));
                        p.circle_stroke(s, 8.0, Stroke::new(1.2, Color32::from_gray(25)));
                        p.text(s, egui::Align2::CENTER_CENTER, k.to_string(), egui::FontId::proportional(10.5), Color32::WHITE);
                    }
                    if resp.drag_started() {
                        if let Some(o) = ui.input(|i| i.pointer.press_origin()) {
                            ae.drag = poly.iter().position(|s| s.distance(o) < 11.0).or_else(|| point_in_poly(o, &poly).then_some(4));
                            ae.last = from(o);
                        }
                    }
                    if let (Some(k), Some(at)) = (ae.drag, resp.interact_pointer_pos().filter(|_| resp.dragged())) {
                        let mut q = from(at);
                        if ae.clamp {
                            q = q.map(|x| x.clamp(0.0, 1.0));
                        }
                        if k == 4 {
                            let d = [q[0] - ae.last[0], q[1] - ae.last[1]];
                            for c in &mut area.pts {
                                *c = [c[0] + d[0], c[1] + d[1]];
                            }
                            ae.last = q;
                        } else if area.mode == 2 {
                            area.pts[k] = q;
                        } else {
                            drag_rect_corner(&mut area.pts, k, q);
                        }
                        changed = true;
                    }
                    if resp.drag_stopped() {
                        ae.drag = None;
                    }
                    resp.on_hover_text("Drag a red corner to move it, or drag inside the area to move all of it");
                });
            });
        });
    if !open {
        st.area_edit = None;
    }
    changed
}

fn material_sets(ui: &mut egui::Ui, doc: &mut Doc, st: &mut EditorState, pics: &Pics) -> bool {
    let mut changed = false;
    let names: Vec<String> = doc.material_sets.iter().map(|s| s.name.clone()).collect();
    let (add, dup, del) = picker(ui, "setpick", &names, "No material sets", &mut st.set);
    if add {
        doc.material_sets.push(MaterialSet::new(&format!("Material Set {}", doc.material_sets.len() + 1)));
        st.set = doc.material_sets.len() - 1;
        changed = true;
    }
    if dup {
        if let Some(s) = doc.material_sets.get(st.set).cloned() {
            doc.material_sets.push(MaterialSet { name: format!("{} copy", s.name), ..s });
            st.set = doc.material_sets.len() - 1;
            changed = true;
        }
    }
    if del && st.set < doc.material_sets.len() {
        doc.material_sets.remove(st.set);
        let fix = |x: &mut Option<usize>, gone: usize| {
            *x = match *x {
                Some(i) if i == gone => None,
                Some(i) if i > gone => Some(i - 1),
                o => o,
            }
        };
        for g in &mut doc.gens {
            fix(&mut g.mat_set, st.set);
            for t in &mut g.types {
                fix(&mut t.set, st.set);
            }
        }
        st.set = st.set.saturating_sub(1);
        changed = true;
    }
    let mat_names: Vec<String> = doc.materials.iter().map(|m| m.name.clone()).collect();
    let mat_cols: Vec<[u8; 3]> = doc.materials.iter().map(|m| m.color()).collect();
    let year = doc.year();
    let si = st.set;
    // A member asking for a brand-new material (made after the set is let go).
    let mut new_mat_for: Option<usize> = None;
    {
        let Some(set) = doc.material_sets.get_mut(si) else {
            ui.add_space(6.0);
            ui.label(RichText::new("Material sets group materials: leaves pick from them by weight, and by season. Add one with ➕.").weak());
            return changed;
        };
        lrow(ui, 64.0, "Name", |ui| {
            changed |= ui.add_sized([(ui.available_width() - 10.0).max(40.0), 22.0], egui::TextEdit::singleline(&mut set.name)).changed();
        });
        boxed(ui, "Settings", |ui| {
            changed |= ui
                .checkbox(&mut set.override_season, "Override season")
                .on_hover_text("On: the members share the year in their order, each blending into the next; the curve maps the year onto the list. Off: each member's own season curve sets when it shows.")
                .changed();
            let w = (ui.available_width() - 6.0).max(60.0);
            changed |= mini_curve(ui, ui.make_persistent_id(("setcurve", si)), &mut set.curve, Vec2::new(w, 46.0), set.override_season, year);
        });
        let mut remove = None;
        let mut rects = vec![];
        boxed(ui, "Members", |ui| {
            if ui.button("➕ Add").on_hover_text("Add a material to the set").clicked() {
                set.members.push(SetMember { material: None, weight: 1.0, season: curve::Curve::flat(1.0) });
                changed = true;
            }
            let over = set.override_season;
            for (i, m) in set.members.iter_mut().enumerate() {
                let r = egui::Frame::new().fill(Color32::from_gray(56)).corner_radius(5).inner_margin(4).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if grip(ui).drag_started() {
                            st.row_drag = Some(("set", i));
                        }
                        // Thumbnail: click to edit the material.
                        let (tr, tresp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
                        checker(ui.painter(), tr, 5.0);
                        if let Some(mi) = m.material {
                            match pics.color.get(mi).and_then(|t| t.as_ref()) {
                                Some(t) => {
                                    ui.painter().image(t.id(), tr, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                                }
                                None => {
                                    let c = mat_cols.get(mi).copied().unwrap_or([128; 3]);
                                    ui.painter().rect_filled(tr, CornerRadius::same(3), Color32::from_rgb(c[0], c[1], c[2]));
                                }
                            }
                        }
                        if tresp.on_hover_text("Edit this material").clicked() {
                            if let Some(mi) = m.material {
                                st.mat = mi;
                                st.tab = 0;
                            }
                        }
                        ui.vertical(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✖").on_hover_text("Remove from the set").clicked() {
                                    remove = Some(i);
                                }
                                if ui.add_enabled(m.material.is_some(), egui::Button::new("✏").small()).on_hover_text("Edit this material").clicked() {
                                    st.mat = m.material.unwrap_or(0);
                                    st.tab = 0;
                                }
                                // The combo's arrow sits outside its width.
                                let w = (ui.available_width() - 30.0).max(40.0);
                                let text = m.material.and_then(|i| mat_names.get(i).cloned()).unwrap_or_else(|| "Choose a material…".into());
                                egui::ComboBox::from_id_salt(("setmem", si, i)).width(w).selected_text(text).show_ui(ui, |ui| {
                                    if ui.selectable_label(false, "➕ New material").on_hover_text("Make a new material for this member").clicked() {
                                        new_mat_for = Some(i);
                                    }
                                    ui.separator();
                                    changed |= ui.selectable_value(&mut m.material, None, "None").changed();
                                    for (k, n) in mat_names.iter().enumerate() {
                                        changed |= ui.selectable_value(&mut m.material, Some(k), n).changed();
                                    }
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.label("Weight");
                                changed |= ui.add(egui::DragValue::new(&mut m.weight).speed(0.01).range(0.0..=10.0)).on_hover_text("How often leaves pick it, against the others").changed();
                                let w = (ui.available_width() - 8.0).max(50.0);
                                let tip = if over { "Override season is on: the set's curve decides when each member shows" } else { "Its weight through the year: drag points (the ends move up and down), click to add, double-click to remove" };
                                changed |= mini_curve(ui, ui.make_persistent_id(("memcurve", si, i)), &mut m.season, Vec2::new(w, 32.0), !over, year);
                                let _ = tip;
                            });
                        });
                    });
                });
                rects.push(r.response.rect);
            }
            if set.members.is_empty() {
                ui.label(RichText::new("No members yet: ➕ Add a material.").weak());
            }
        });
        if let Some((from, to)) = reorder(ui, &mut st.row_drag, "set", &rects) {
            let m = set.members.remove(from);
            set.members.insert(to, m);
            changed = true;
        }
        if let Some(i) = remove {
            set.members.remove(i);
            changed = true;
        }
    }
    if let Some(i) = new_mat_for {
        doc.materials.push(Material::new(&format!("Material {}", doc.materials.len() + 1), [120, 160, 80], 1));
        let idx = doc.materials.len() - 1;
        if let Some(m) = doc.material_sets.get_mut(si).and_then(|s| s.members.get_mut(i)) {
            m.material = Some(idx);
        }
        changed = true;
    }
    changed
}

/// A small curve edited in place: drag points (the end points move up and down), click empty
/// space to add one, double-click a middle point to remove it. Grey and fixed when not
/// `enabled`. A faint line marks `now` (the time of year).
fn mini_curve(ui: &mut egui::Ui, id: egui::Id, c: &mut curve::Curve, size: Vec2, enabled: bool, now: f32) -> bool {
    let (r, resp) = ui.allocate_exact_size(size, if enabled { Sense::click_and_drag() } else { Sense::hover() });
    let p = ui.painter_at(r);
    p.rect_filled(r, CornerRadius::same(4), Color32::from_gray(if enabled { 34 } else { 44 }));
    let col = if enabled { Color32::from_rgb(110, 200, 120) } else { Color32::from_gray(110) };
    let top = c.pts.iter().map(|q| q[1]).fold(1.0f32, f32::max).max(1.0);
    let area = r.shrink(4.0);
    let to = |q: [f32; 2]| Pos2::new(area.left() + q[0] * area.width(), area.bottom() - q[1] / top * area.height());
    let from = |s: Pos2| [((s.x - area.left()) / area.width()).clamp(0.0, 1.0), ((area.bottom() - s.y) / area.height() * top).clamp(0.0, 4.0)];
    let x = area.left() + now.clamp(0.0, 1.0) * area.width();
    p.line_segment([Pos2::new(x, area.top()), Pos2::new(x, area.bottom())], Stroke::new(1.0, Color32::from_white_alpha(40)));
    let line: Vec<Pos2> = (0..=48).map(|i| to([i as f32 / 48.0, c.eval(i as f32 / 48.0)])).collect();
    p.add(egui::Shape::line(line, Stroke::new(1.6, col)));
    for q in &c.pts {
        p.circle_filled(to(*q), 3.5, col);
    }
    if !enabled {
        return false;
    }
    let mut changed = false;
    let hit = |pts: &[[f32; 2]], at: Pos2| pts.iter().position(|q| to(*q).distance(at) < 8.0);
    if resp.drag_started() {
        let h = ui.input(|i| i.pointer.press_origin()).and_then(|o| hit(&c.pts, o));
        ui.data_mut(|d| d.insert_temp(id, h));
    }
    if resp.dragged() {
        if let (Some(i), Some(at)) = (ui.data(|d| d.get_temp::<Option<usize>>(id)).flatten(), resp.interact_pointer_pos()) {
            let n = c.pts.len();
            if i < n {
                let q = from(at);
                let end = i == 0 || i + 1 == n;
                let lo = if i == 0 { 0.0 } else { c.pts[i - 1][0] + 0.001 };
                let hi = if i + 1 == n { 1.0 } else { c.pts[i + 1][0] - 0.001 };
                c.pts[i] = [if end { c.pts[i][0] } else { q[0].clamp(lo, hi) }, q[1]];
                changed = true;
            }
        }
    }
    if resp.drag_stopped() {
        ui.data_mut(|d| d.insert_temp::<Option<usize>>(id, None));
    }
    if let Some(at) = resp.interact_pointer_pos() {
        if resp.double_clicked() {
            if let Some(i) = hit(&c.pts, at).filter(|&i| i > 0 && i + 1 < c.pts.len()) {
                c.pts.remove(i);
                changed = true;
            }
        } else if resp.clicked() && hit(&c.pts, at).is_none() {
            let q = from(at);
            c.pts.push(q);
            c.sort();
            changed = true;
        }
    }
    resp.on_hover_text("Drag points (the end points move up and down) · click to add a point · double-click one to remove it");
    changed
}

fn displacements(ui: &mut egui::Ui, doc: &mut Doc, st: &mut EditorState) -> bool {
    let mut changed = false;
    let names: Vec<String> = doc.displacements.iter().map(|d| d.name.clone()).collect();
    let (add, dup, del) = picker(ui, "disppick", &names, "No displacements", &mut st.disp);
    if add {
        doc.displacements.push(DispAsset::new(&format!("Displacement {}", doc.displacements.len() + 1)));
        st.disp = doc.displacements.len() - 1;
        changed = true;
    }
    if dup {
        if let Some(d) = doc.displacements.get(st.disp).cloned() {
            doc.displacements.push(DispAsset { name: format!("{} copy", d.name), ..d });
            st.disp = doc.displacements.len() - 1;
            changed = true;
        }
    }
    if del && st.disp < doc.displacements.len() {
        doc.displacements.remove(st.disp);
        for g in &mut doc.gens {
            g.disp_map = match g.disp_map {
                Some(i) if i == st.disp => None,
                Some(i) if i > st.disp => Some(i - 1),
                o => o,
            };
        }
        st.disp = st.disp.saturating_sub(1);
        changed = true;
    }
    let Some(d) = doc.displacements.get_mut(st.disp) else {
        ui.add_space(6.0);
        ui.label(RichText::new("Add a displacement with ➕ and pick a picture. Then use it in a branch's Displacement tab ▸ Map.").weak());
        return changed;
    };
    boxed(ui, "Settings", |ui| {
        lrow(ui, 76.0, "Name", |ui| changed |= ui.add_sized([(ui.available_width() - 10.0).max(40.0), 22.0], egui::TextEdit::singleline(&mut d.name)).changed());
        // Source: a picture file, or a gradient or noise made here (texgen), like the material maps.
        let mut src = match d.gen.source {
            2 => 1,
            3 => 2,
            _ => 0,
        };
        lrow(ui, 76.0, "Source", |ui| {
            if combo(ui, "dispsrc", &mut src, &["File", "Gradient", "Noise"], (ui.available_width() - 40.0).clamp(60.0, 140.0)) {
                d.gen.source = [0, 2, 3][src];
                changed = true;
            }
            if d.gen.source >= 2 {
                ui.menu_button("✨", |ui| {
                    for (name, g) in texgen::presets() {
                        if g.source == d.gen.source && ui.button(name).clicked() {
                            d.gen = g;
                            changed = true;
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("Presets");
            }
        });
        if d.gen.source == 2 {
            lrow(ui, 76.0, "Type", |ui| changed |= combo(ui, "dispgrad", &mut d.gen.gradient, &texgen::GRADIENTS, (ui.available_width() - 10.0).clamp(60.0, 140.0)));
            for (k, name) in ["Start", "End"].iter().enumerate() {
                lrow(ui, 76.0, name, |ui| {
                    let pt = if k == 0 { &mut d.gen.p0 } else { &mut d.gen.p1 };
                    for v in pt.iter_mut() {
                        changed |= ui.add(egui::DragValue::new(v).speed(0.005).range(-1.0..=2.0).max_decimals(3)).changed();
                    }
                });
            }
        }
        if d.gen.source == 3 {
            lrow(ui, 76.0, "Type", |ui| changed |= combo(ui, "dispnoise", &mut d.gen.noise, &texgen::NOISES, (ui.available_width() - 10.0).clamp(60.0, 140.0)));
            lrow(ui, 76.0, "Scale", |ui| changed |= ui.add(egui::Slider::new(&mut d.gen.scale, 0.5..=40.0).logarithmic(true)).on_hover_text("How many cells or blobs across").changed());
            lrow(ui, 76.0, "Detail", |ui| changed |= ui.add(egui::Slider::new(&mut d.gen.detail, 1..=8)).on_hover_text("Layers of finer noise").changed());
            lrow(ui, 76.0, "Pan", |ui| {
                for v in d.gen.offset.iter_mut() {
                    changed |= ui.add(egui::DragValue::new(v).speed(0.005).max_decimals(3)).on_hover_text("Slide the pattern").changed();
                }
            });
            lrow(ui, 76.0, "Seed", |ui| {
                changed |= ui.add(egui::DragValue::new(&mut d.gen.seed)).changed();
                if ui.small_button("🎲").on_hover_text("New random pattern").clicked() {
                    d.gen.seed = d.gen.seed.wrapping_mul(2_654_435_761).wrapping_add(97) % 100_000;
                    changed = true;
                }
            });
        }
        if d.gen.source == 0 {
        lrow(ui, 76.0, "Filename", |ui| {
            let mut path = d.file.clone().unwrap_or_default();
            ui.add_sized([(ui.available_width() - 76.0).max(40.0), 22.0], egui::TextEdit::singleline(&mut path).interactive(false)).on_hover_text("The picture (light is out, dark is in)");
            if ui.button("…").on_hover_text("Choose a picture").clicked() {
                if let Some(f) = pick_image() {
                    d.file = Some(f);
                    changed = true;
                }
            }
            if ui.button("⟳").on_hover_text("Read the picture again").clicked() {
                texgen::clear_disp_cache();
                st.disp_tex = None;
                changed = true;
            }
        });
        lrow(ui, 76.0, "Channel", |ui| changed |= combo(ui, "dispch", &mut d.channel, &["Gray", "Red", "Green", "Blue", "Alpha"], (ui.available_width() - 10.0).clamp(60.0, 140.0)));
        }
        lrow(ui, 76.0, "Smoothing", |ui| changed |= ui.add(egui::Slider::new(&mut d.smoothing, 0.0..=1.0)).on_hover_text("Soften the bumps").changed());
        lrow(ui, 76.0, "Brightness", |ui| changed |= ui.add(egui::Slider::new(&mut d.brightness, -1.0..=1.0)).on_hover_text("Push everything out (or in)").changed());
        lrow(ui, 76.0, "Contrast", |ui| changed |= ui.add(egui::Slider::new(&mut d.contrast, -1.0..=1.0)).on_hover_text("Deeper or shallower bumps").changed());
        lrow(ui, 76.0, "Normalize", |ui| changed |= ui.checkbox(&mut d.normalize, "").on_hover_text("Stretch the heights to use the full range").changed());
    });
    ui.add(egui::Label::new(RichText::new("Use it in a branch or trunk: Displacement tab ▸ Map. Amount and the U/V scales still apply.").size(11.5).weak()).wrap());
    let side = (ui.available_width() - 4.0).min(ui.available_height() - 8.0).max(80.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new((ui.available_width() - 4.0).max(80.0), side), Sense::hover());
    let sq = Rect::from_center_size(r.center(), Vec2::splat(side));
    checker(ui.painter(), r, 10.0);
    let key = serde_json::to_string(&*d).unwrap_or_default();
    if st.disp_tex.as_ref().is_none_or(|(k, _)| *k != key) && !ui.input(|i| i.pointer.any_down()) {
        st.disp_tex = texgen::disp_heights(d).map(|m| (key, ui.ctx().load_texture("disp_preview", to_color_image(&m.image(), 256), egui::TextureOptions::LINEAR)));
    }
    match &st.disp_tex {
        Some((_, t)) => {
            ui.painter().image(t.id(), sq, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            ui.painter().text(sq.center(), egui::Align2::CENTER_CENTER, "No picture", egui::FontId::proportional(13.0), Color32::from_gray(200));
        }
    }
    changed
}

// ---------------------------------------------------------------- cutout drawing

/// View of card space (0..1, y up) inside a screen rect, with zoom and pan.
#[derive(Clone, Copy)]
struct View {
    r: Rect,
    zoom: f32,
    pan: Vec2,
}

impl View {
    fn side(&self) -> f32 {
        self.r.width().min(self.r.height()) * 0.86 * self.zoom
    }
    fn card(&self) -> Rect {
        Rect::from_center_size(self.r.center() + self.pan, Vec2::splat(self.side()))
    }
    fn to(&self, q: [f32; 2]) -> Pos2 {
        let c = self.card();
        Pos2::new(c.left() + q[0] * c.width(), c.bottom() - q[1] * c.height())
    }
    fn from(&self, p: Pos2) -> [f32; 2] {
        let c = self.card();
        [(p.x - c.left()) / c.width(), (c.bottom() - p.y) / c.height()]
    }
}

fn draw_cutout(p: &egui::Painter, v: &View, m: &MeshAsset, lod: usize, line: Color32, handles: bool, sel: &[usize]) {
    let (pts, tris) = cutout::build(m, lod);
    for t in &tris {
        let q: Vec<Pos2> = t.iter().map(|&k| v.to(pts[k])).collect();
        for k in 0..3 {
            p.line_segment([q[k], q[(k + 1) % 3]], Stroke::new(1.3, line));
        }
    }
    let orange = Color32::from_rgb(240, 150, 40);
    let piv = v.to(m.pivot);
    p.line_segment([Pos2::new(piv.x, v.card().top()), piv], Stroke::new(1.5, orange.gamma_multiply(0.8)));
    if handles {
        for mk in &m.mask[lod] {
            let c = v.to([mk[0], mk[1]]);
            p.circle_filled(c, mk[2] * v.card().width(), Color32::from_rgba_unmultiplied(220, 40, 40, 50));
        }
        for (i, q) in m.lods[lod].iter().enumerate() {
            let col = if sel.contains(&i) { Color32::YELLOW } else if lod == 0 { m.colors.get(i).map_or(Color32::from_rgb(225, 30, 30), |c| Color32::from_rgb(c[0], c[1], c[2])) } else { Color32::from_rgb(225, 30, 30) };
            let s = v.to(*q);
            p.circle_filled(s, 5.5, col);
            p.circle_stroke(s, 5.5, Stroke::new(1.0, Color32::from_rgb(120, 0, 0)));
        }
        for a in &m.anchors {
            let s = v.to(a.pos);
            let e = v.to([a.pos[0] + a.dir[0], a.pos[1] + a.dir[1]]);
            p.line_segment([s, e], Stroke::new(1.5, Color32::WHITE));
            p.circle_filled(e, 3.5, Color32::WHITE);
            p.circle_filled(s, 5.5, Color32::from_rgb(60, 200, 90));
            p.text(s + Vec2::new(7.0, -7.0), egui::Align2::LEFT_BOTTOM, a.id.to_string(), egui::FontId::proportional(11.0), Color32::from_rgb(60, 220, 90));
        }
        p.circle_filled(piv, 7.0, Color32::WHITE);
        p.circle_stroke(piv, 7.0, Stroke::new(1.5, Color32::from_gray(40)));
    }
}

// ---------------------------------------------------------------- mesh files

/// A copy of the mesh whose `lod` builds unturned, for card-space uv.
fn unturned(m: &MeshAsset, lod: usize) -> MeshAsset {
    let mut flat = m.clone();
    let c = m.cfg(lod);
    flat.slot_cfg[lod.min(2)] = Some(SlotCfg { angle: 0.0, ..c });
    flat
}

/// Write one LOD of a cutout as an OBJ mesh: flat (z = 0), measured from the pivot and turned by
/// its angle, with each point's place on the picture as its uv.
pub fn export_obj(m: &MeshAsset, lod: usize, path: &std::path::Path) -> std::io::Result<()> {
    let c = m.cfg(lod);
    let (uv, tris) = cutout::build(&unturned(m, lod), lod);
    let mut s = format!("# TreeCraft cutout mesh: {} (LOD {lod})\n", m.name);
    for q in &uv {
        let p = cutout::rotate(c.angle, c.pivot, *q);
        s += &format!("v {:.6} {:.6} 0\n", p[0] - c.pivot[0], p[1] - c.pivot[1]);
    }
    for q in &uv {
        s += &format!("vt {:.6} {:.6}\n", q[0], q[1]);
    }
    s += "vn 0 0 1\n";
    for t in &tris {
        s += &format!("f {0}/{0}/1 {1}/{1}/1 {2}/{2}/1\n", t[0] + 1, t[1] + 1, t[2] + 1);
    }
    std::fs::write(path, s)
}

/// Read an OBJ mesh's points onto the card: their uv when every point has one, else the mesh's
/// two widest axes scaled into the card with its lowest point at the stem. The points are joined
/// into triangles again like any cutout (the file's own faces aren't kept).
pub fn import_obj(path: &std::path::Path) -> Result<Vec<[f32; 2]>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let (mut v, mut vt): (Vec<[f32; 3]>, Vec<[f32; 2]>) = (vec![], vec![]);
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let tag = it.next();
        let n: Vec<f32> = it.filter_map(|x| x.parse().ok()).collect();
        match tag {
            Some("v") if n.len() >= 3 => v.push([n[0], n[1], n[2]]),
            Some("vt") if n.len() >= 2 => vt.push([n[0], n[1]]),
            _ => {}
        }
    }
    let pts: Vec<[f32; 2]> = if !vt.is_empty() && vt.len() == v.len() {
        vt
    } else {
        if v.len() < 3 {
            return Err("The file has fewer than three points.".into());
        }
        let (lo, hi) = v.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(l, h), p| ([l[0].min(p[0]), l[1].min(p[1]), l[2].min(p[2])], [h[0].max(p[0]), h[1].max(p[1]), h[2].max(p[2])]));
        let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        let mut ax = [0usize, 1, 2];
        ax.sort_by(|a, b| ext[*b].total_cmp(&ext[*a]));
        let (a, b) = (ax[0].min(ax[1]), ax[0].max(ax[1]));
        let s = ext[a].max(ext[b]).max(1e-6);
        v.iter().map(|p| [0.5 + (p[a] - (lo[a] + hi[a]) * 0.5) / s, (p[b] - lo[b]) / s]).collect()
    };
    let mut out: Vec<[f32; 2]> = vec![];
    for p in pts {
        if !out.iter().any(|q| (q[0] - p[0]).abs() < 1e-5 && (q[1] - p[1]).abs() < 1e-5) {
            out.push(p);
        }
    }
    if out.len() < 3 {
        return Err("The file has fewer than three different points.".into());
    }
    Ok(out)
}

fn obj_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new().add_filter("Wavefront OBJ mesh", &["obj"])
}

// ---------------------------------------------------------------- the Meshes tab

/// The leaf in 3D: turned by yaw and pitch, with its pivot, anchors and axes.
fn leaf_3d(p: &egui::Painter, r: Rect, m: &MeshAsset, lod: usize, tex: Option<&egui::TextureHandle>, yaw: f32, pitch: f32) {
    p.rect_filled(r, CornerRadius::same(6), Color32::from_gray(34));
    let c = m.cfg(lod);
    if m.lods[lod.min(2)].len() < 3 {
        p.text(r.center(), egui::Align2::CENTER_CENTER, "Empty mesh\nEdit a slot to draw a cutout, or import a mesh", egui::FontId::proportional(12.5), Color32::from_gray(150));
        return;
    }
    let (uv, tris) = cutout::build(&unturned(m, lod), lod);
    let k = m.scale.clamp(0.05, 3.0);
    let world = |q: [f32; 2]| {
        let t = cutout::rotate(c.angle, c.pivot, q);
        cutout::axes(m.orient, m.flip, m.growth, [t[0] - c.pivot[0], t[1] - c.pivot[1], 0.0]).map(|x| x * k)
    };
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let view = |w: [f32; 3]| {
        let x = w[0] * cy + w[2] * sy;
        let z = -w[0] * sy + w[2] * cy;
        (x, w[1] * cp - z * sp, w[1] * sp + z * cp)
    };
    let scale = r.height().min(r.width()) * 0.72;
    let origin = r.center() + Vec2::new(0.0, scale * 0.42);
    let to = |w: [f32; 3]| {
        let (x, y, z) = view(w);
        let k = 1.0 / (1.0 + z * 0.3).max(0.2);
        origin + Vec2::new(x * k, -y * k) * scale
    };
    // A floor grid under the pivot.
    for i in -4..=4 {
        let f = i as f32 * 0.125;
        let col = Color32::from_gray(if i == 0 { 70 } else { 52 });
        p.line_segment([to([f, 0.0, -0.5]), to([f, 0.0, 0.5])], Stroke::new(1.0, col));
        p.line_segment([to([-0.5, 0.0, f]), to([0.5, 0.0, f])], Stroke::new(1.0, col));
    }
    let facing = view(cutout::front(m.orient, m.flip, m.growth, m.flip_normals)).2 < 0.0;
    let shade = if facing { Color32::WHITE } else { Color32::from_gray(150) };
    let mut mesh = match tex {
        Some(t) => egui::Mesh::with_texture(t.id()),
        None => egui::Mesh::default(),
    };
    for q in &uv {
        let col = if tex.is_some() { shade } else if facing { Color32::from_rgb(110, 175, 90) } else { Color32::from_rgb(70, 120, 60) };
        let tuv = if tex.is_some() { Pos2::new(q[0], 1.0 - q[1]) } else { egui::epaint::WHITE_UV };
        mesh.vertices.push(egui::epaint::Vertex { pos: to(world(*q)), uv: tuv, color: col });
    }
    for t in &tris {
        mesh.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
    }
    p.add(egui::Shape::mesh(mesh));
    for t in &tris {
        let q: Vec<Pos2> = t.iter().map(|&k| to(world(uv[k]))).collect();
        for k in 0..3 {
            p.line_segment([q[k], q[(k + 1) % 3]], Stroke::new(0.7, Color32::from_rgba_unmultiplied(70, 220, 240, 110)));
        }
    }
    // Axes from the pivot, then anchors and the pivot itself.
    for (dir, col, name) in [([0.25, 0.0, 0.0], Color32::from_rgb(235, 80, 80), "X"), ([0.0, 0.25, 0.0], Color32::from_rgb(90, 210, 90), "Y"), ([0.0, 0.0, 0.25], Color32::from_rgb(90, 140, 250), "Z")] {
        let (a, b) = (to([0.0; 3]), to(dir));
        p.line_segment([a, b], Stroke::new(2.0, col));
        p.text(b, egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(11.5), col);
    }
    let green = Color32::from_rgb(70, 215, 110);
    for a in &c.anchors {
        let (s, e) = (to(world(a.pos)), to(world([a.pos[0] + a.dir[0], a.pos[1] + a.dir[1]])));
        p.line_segment([s, e], Stroke::new(2.0, green));
        p.circle_filled(s, 4.5, green);
    }
    let o = to([0.0; 3]);
    p.circle_filled(o, 5.5, Color32::from_gray(25));
    p.circle_stroke(o, 5.5, Stroke::new(2.0, Color32::from_rgb(250, 160, 50)));
    p.text(r.left_bottom() + Vec2::new(8.0, -6.0), egui::Align2::LEFT_BOTTOM, "Drag to turn · double-click to reset", egui::FontId::proportional(10.5), Color32::from_gray(140));
}

const LOD_NAMES: [&str; 3] = ["High", "Medium", "Low"];

fn meshes(ui: &mut egui::Ui, doc: &mut Doc, st: &mut EditorState, pics: &Pics) -> bool {
    let mut changed = false;
    let names: Vec<String> = doc.meshes.iter().map(|m| m.name.clone()).collect();
    let (add, dup, del) = picker(ui, "meshpick", &names, "No meshes", &mut st.mesh);
    if add || dup {
        let mut m = if dup { doc.meshes.get(st.mesh).cloned().unwrap_or_else(MeshAsset::leaf_card) } else { MeshAsset::leaf_card() };
        m.name = format!("Mesh {}", doc.meshes.len() + 1);
        doc.meshes.push(m);
        st.mesh = doc.meshes.len() - 1;
        changed = true;
    }
    if del && doc.meshes.len() > 1 && st.mesh < doc.meshes.len() {
        doc.meshes.remove(st.mesh);
        for g in &mut doc.gens {
            g.mesh = match g.mesh {
                Some(i) if i == st.mesh => None,
                Some(i) if i > st.mesh => Some(i - 1),
                x => x,
            };
        }
        for mt in &mut doc.materials {
            mt.cut_rows.retain(|r| *r != Some(st.mesh));
            for r in mt.cut_rows.iter_mut().flatten() {
                if *r > st.mesh {
                    *r -= 1;
                }
            }
            mt.cutout = match mt.cutout {
                Some(i) if i == st.mesh => None,
                Some(i) if i > st.mesh => Some(i - 1),
                x => x,
            };
        }
        st.mesh = st.mesh.saturating_sub(1);
        changed = true;
    }
    let mat_names: Vec<String> = doc.materials.iter().map(|m| m.name.clone()).collect();
    let mesh_i = st.mesh;
    let Some(m) = doc.meshes.get_mut(mesh_i) else { return changed };
    lrow(ui, 64.0, "Name", |ui| changed |= ui.add(egui::TextEdit::singleline(&mut m.name).desired_width((ui.available_width() - 8.0).max(40.0))).changed());
    lrow(ui, 64.0, "Material", |ui| {
        let mut opts = vec!["None"];
        opts.extend(mat_names.iter().map(String::as_str));
        let mut i = m.material.map_or(0, |x| x + 1);
        if combo(ui, "meshmat", &mut i, &opts, ui.available_width() - 8.0) {
            m.material = i.checked_sub(1);
            changed = true;
        }
    });
    let tex = m.material.and_then(|i| pics.color.get(i)).and_then(|t| t.as_ref());
    // 3D view of the leaf.
    let h = (ui.available_height() * 0.4).clamp(160.0, 340.0);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click_and_drag());
    if resp.dragged() {
        let d = resp.drag_delta();
        st.view_yaw += d.x * 0.012;
        st.view_pitch = (st.view_pitch + d.y * 0.012).clamp(-1.4, 1.0);
    }
    if resp.double_clicked() {
        st.view_yaw = 0.0;
        st.view_pitch = 0.0;
    }
    leaf_3d(&ui.painter_at(r), r, m, st.mesh_show, tex, st.view_yaw + 0.5, st.view_pitch + 0.3);
    resp.on_hover_text(format!("The {} LOD in 3D: orange pivot, green anchors, X/Y/Z axes. Drag to turn it.", LOD_NAMES[st.mesh_show.min(2)]));
    // The three LOD slots.
    ui.add_space(4.0);
    let w = ((ui.available_width() - 12.0) / 3.0).max(60.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (l, name) in LOD_NAMES.iter().enumerate() {
            let shown = st.mesh_show == l;
            egui::Frame::new()
                .fill(Color32::from_gray(if shown { 54 } else { 42 }))
                .stroke(Stroke::new(1.0, if shown { Color32::from_rgb(0x47, 0x72, 0xb3) } else { Color32::from_gray(58) }))
                .corner_radius(6)
                .inner_margin(4)
                .show(ui, |ui| {
                    ui.set_width(w - 10.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(*name).size(12.0).strong());
                        let (pr, presp) = ui.allocate_exact_size(Vec2::splat(w - 14.0), Sense::click());
                        let pp = ui.painter_at(pr);
                        pp.rect_filled(pr, CornerRadius::same(4), Color32::from_gray(28));
                        let pv = View { r: pr, zoom: 1.0, pan: Vec2::ZERO };
                        if m.lods[l].len() >= 3 {
                            checker(&pp, pv.card(), 5.0);
                            if let Some(t) = tex {
                                pp.image(t.id(), pv.card(), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            }
                            draw_cutout(&pp, &pv, m, l, Color32::from_rgb(70, 220, 240), false, &[]);
                        } else {
                            pp.text(pr.center(), egui::Align2::CENTER_CENTER, "Empty\n(uses High)", egui::FontId::proportional(11.0), Color32::from_gray(140));
                        }
                        if presp.on_hover_text(format!("Show the {name} LOD in the 3D view")).clicked() {
                            st.mesh_show = l;
                        }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            if icon_btn(ui, "✏", &format!("Edit the {name} LOD in the Cutout Editor")) {
                                st.edit_mesh = Some(MeshEdit::open(mesh_i, m.clone(), l));
                            }
                            if icon_btn(ui, "📂", &format!("Import a mesh into the {name} slot (or convert its cutout to a mesh file)")) {
                                st.import_ask = Some(l);
                            }
                            if icon_btn(ui, "🗑", &format!("Empty the {name} slot (it then uses High)")) {
                                m.lods[l].clear();
                                m.mask[l].clear();
                                m.slot_cfg[l] = None;
                                changed = true;
                            }
                        });
                    });
                });
        }
    });
    // Import into a slot: convert the cutout to a mesh file, or replace it with a mesh.
    if let Some(l) = st.import_ask {
        let mut open = true;
        egui::Window::new(format!("{} slot", LOD_NAMES[l])).id(egui::Id::unique("mesh_import_ask")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).open(&mut open).show(ui.ctx(), |ui| {
            ui.label("This slot holds a cutout. Do you want to convert the cutout to a mesh, or import a mesh?");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Convert to mesh…").on_hover_text("Save this slot's cutout as an OBJ mesh file").clicked() {
                    if let Some(path) = obj_dialog().set_file_name(format!("{} {}.obj", m.name, LOD_NAMES[l])).save_file() {
                        st.note = Some(match export_obj(m, l, &path) {
                            Ok(()) => format!("Saved {}.", path.display()),
                            Err(e) => format!("Couldn't save the mesh: {e}"),
                        });
                    }
                    st.import_ask = None;
                }
                if ui.button("Import mesh…").on_hover_text("Pick an OBJ mesh for this slot; closing the picker leaves the slot empty").clicked() {
                    match obj_dialog().pick_file() {
                        Some(path) => match import_obj(&path) {
                            Ok(pts) => {
                                m.lods[l] = pts;
                                m.mask[l].clear();
                                st.note = Some(format!("Imported {} into the {} slot.", path.display(), LOD_NAMES[l]));
                            }
                            Err(e) => st.note = Some(format!("Couldn't import the mesh: {e}")),
                        },
                        None => {
                            m.lods[l].clear();
                            m.mask[l].clear();
                            st.note = Some(format!("The {} slot is empty now (it uses High).", LOD_NAMES[l]));
                        }
                    }
                    changed = true;
                    st.import_ask = None;
                }
                if ui.button("Cancel").clicked() {
                    st.import_ask = None;
                }
            });
        });
        if !open {
            st.import_ask = None;
        }
    }
    if let Some(msg) = &st.note {
        if st.edit_mesh.is_none() {
            ui.label(RichText::new(msg).size(11.5).color(Color32::from_rgb(240, 190, 90)));
        }
    }
    ui.add_space(4.0);
    let half = (ui.available_width() - 8.0) / 2.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(half);
            boxed(ui, "Transform", |ui| {
                lrow(ui, 76.0, "Orient", |ui| changed |= combo(ui, "orient", &mut m.orient, &["Z-Up RH", "Y-Up RH", "Y-Up LH", "Z-Up LH"], ui.available_width() - 6.0));
                lrow(ui, 76.0, "Flip", |ui| changed |= combo(ui, "flip", &mut m.flip, &["X Y Z", "-X Y Z", "X -Y Z", "X Y -Z"], ui.available_width() - 6.0));
                lrow(ui, 76.0, "Flip normals", |ui| changed |= ui.checkbox(&mut m.flip_normals, "").changed());
                lrow(ui, 76.0, "Fix winding", |ui| changed |= ui.checkbox(&mut m.fix_winding, "").changed());
            });
        });
        ui.vertical(|ui| {
            ui.set_width(half);
            boxed(ui, "Wind direction", |ui| {
                lrow(ui, 54.0, "Growth", |ui| changed |= combo(ui, "growth", &mut m.growth, &["+Y", "+X", "-X", "+Z", "-Z"], ui.available_width() - 6.0));
                lrow(ui, 54.0, "Ripple", |ui| changed |= combo(ui, "ripple", &mut m.ripple, &["Normal", "None", "Strong"], ui.available_width() - 6.0));
            });
            boxed(ui, "Scale", |ui| {
                lrow(ui, 54.0, "Value", |ui| changed |= ui.add(egui::DragValue::new(&mut m.scale).speed(0.01).range(0.05..=10.0)).changed());
                ui.vertical_centered_justified(|ui| {
                    if ui.button("Compute").on_hover_text("Set the scale from how much of the picture the cutout covers").clicked() {
                        let (pp, _) = cutout::build(m, 0);
                        let (lo, hi) = pp.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(l, h), q| ([l[0].min(q[0]), l[1].min(q[1])], [h[0].max(q[0]), h[1].max(q[1])]));
                        m.scale = ((hi[0] - lo[0]).max(hi[1] - lo[1])).clamp(0.05, 10.0);
                        changed = true;
                    }
                });
            });
        });
    });
    changed
}

// ---------------------------------------------------------------- the Cutout Editor

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    /// Click adds a point, double-click a point deletes it; drag moves points, the pivot or its arrow.
    Point,
    /// Click or drag to place an anchor; drag one to edit it; double-click deletes it.
    Anchor,
    /// Paint see-through spots; Ctrl, or double-click and hold, erases.
    Brush,
}

/// The shape a drag on empty space selects points with.
#[derive(Clone, Copy, PartialEq)]
pub enum SelMode {
    Box,
    Circle,
    Lasso,
}

/// A question waiting in the Cutout Editor because the scene has changes no slot holds.
#[derive(Clone, Copy, PartialEq)]
enum Ask {
    /// Bring this slot into the scene over the changes.
    Load(usize),
    /// Leave the editor; true keeps the slots (Done), false drops everything (Cancel).
    Close(bool),
}

/// The Cutout Editor's state. The scene (the big canvas) is a working copy: edits stay there
/// until ➡ puts them into an LOD slot, and ⬅ brings a slot back. Done writes the slots back.
pub struct MeshEdit {
    pub index: usize,
    /// The mesh with its three LOD slots.
    pub mesh: MeshAsset,
    /// The working cutout: its High points, mask and settings are what the scene shows.
    pub scene: MeshAsset,
    /// The slot the scene last came from or went to.
    pub lod: usize,
    /// The scene has changes no slot holds.
    dirty: bool,
    ask: Option<Ask>,
    pub tool: Tool,
    pub sel_mode: SelMode,
    pub mirror: bool,
    pub selected: Vec<usize>,
    drag: Option<Drag>,
    sel_path: Vec<Pos2>,
    pub density: usize,
    pub brush_screen: bool,
    pub brush_size: f32,
    erase_stroke: bool,
    last_press: f64,
    /// Which of the material's maps is behind the scene.
    pub bg_map: usize,
    maps: HashMap<(usize, usize), Option<egui::TextureHandle>>,
    zoom: f32,
    pan: Vec2,
    history: Vec<MeshAsset>,
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Points([f32; 2]),
    Pivot,
    Angle,
    Anchor(usize),
    AnchorDir(usize),
    Select,
    Paint,
}

/// Length of the pivot's direction arrow, in card units.
const ARROW: f32 = 0.22;

impl MeshEdit {
    pub fn new(index: usize, mesh: MeshAsset) -> MeshEdit {
        MeshEdit::open(index, mesh, 0)
    }
    /// Open the editor with `slot` in the scene.
    pub fn open(index: usize, mesh: MeshAsset, slot: usize) -> MeshEdit {
        let mut ed = MeshEdit {
            index,
            scene: mesh.clone(),
            mesh,
            lod: 0,
            dirty: false,
            ask: None,
            tool: Tool::Point,
            sel_mode: SelMode::Box,
            mirror: false,
            selected: vec![],
            drag: None,
            sel_path: vec![],
            density: 24,
            brush_screen: false,
            brush_size: 0.06,
            erase_stroke: false,
            last_press: -1.0,
            bg_map: 0,
            maps: HashMap::new(),
            zoom: 1.0,
            pan: Vec2::ZERO,
            history: vec![],
        };
        ed.load(slot.min(2));
        ed
    }
    /// Bring a slot's points, paint, anchors and settings into the scene.
    fn load(&mut self, slot: usize) {
        let pts = if self.mesh.lods[slot].len() >= 3 { self.mesh.lods[slot].clone() } else { self.mesh.lods[0].clone() };
        let mask = if self.mesh.lods[slot].len() >= 3 { self.mesh.mask[slot].clone() } else { self.mesh.mask[0].clone() };
        let cfg = self.mesh.cfg(slot);
        let s = &mut self.scene;
        s.lods = [pts, vec![], vec![]];
        s.mask = [mask, vec![], vec![]];
        s.slot_cfg = Default::default();
        s.apply(&cfg);
        s.material = self.mesh.material;
        self.lod = slot;
        self.dirty = false;
        self.selected.clear();
    }
    /// Put the scene into a slot: its points, paint, anchors and settings, kept from now on.
    fn store(&mut self, slot: usize) {
        self.mesh.lods[slot] = self.scene.lods[0].clone();
        self.mesh.mask[slot] = self.scene.mask[0].clone();
        let cfg = self.scene.cfg(0);
        if slot == 0 {
            self.mesh.apply(&cfg);
            self.mesh.colors = self.scene.colors.clone();
        }
        self.mesh.slot_cfg[slot] = Some(cfg);
        self.mesh.material = self.scene.material;
        self.lod = slot;
        self.dirty = false;
    }
    fn save_undo(&mut self) {
        self.history.push(self.scene.clone());
        if self.history.len() > 60 {
            self.history.remove(0);
        }
        self.dirty = true;
    }
    fn pts(&mut self) -> &mut Vec<[f32; 2]> {
        &mut self.scene.lods[0]
    }
    /// The tip of the pivot's arrow: the leaf's growth direction (Angle, turned about the pivot).
    fn arrow_tip(&self) -> [f32; 2] {
        let a = self.scene.angle.to_radians();
        [self.scene.pivot[0] - a.sin() * ARROW, self.scene.pivot[1] + a.cos() * ARROW]
    }
    fn remove_point(&mut self, i: usize) {
        if i < self.scene.lods[0].len() {
            self.scene.lods[0].remove(i);
            if i < self.scene.colors.len() {
                self.scene.colors.remove(i);
            }
        }
        self.selected.clear();
    }
    fn brush_r(&self) -> f32 {
        if self.brush_screen { self.brush_size / self.zoom } else { self.brush_size }
    }
    /// One of the material's maps as a texture (loaded once, then kept).
    fn map_tex(&mut self, ctx: &egui::Context, doc: &Doc, pics: &Pics, mat: usize, slot: usize) -> Option<egui::TextureHandle> {
        let from_pics = match slot {
            0 => pics.color.get(mat).cloned().flatten(),
            1 => pics.alpha.get(mat).cloned().flatten(),
            _ => None,
        };
        if from_pics.is_some() {
            return from_pics;
        }
        self.maps
            .entry((mat, slot))
            .or_insert_with(|| {
                let s = doc.materials.get(mat)?.maps.get(slot).filter(|s| s.on)?;
                let img = slot_image(s, 512)?;
                Some(ctx.load_texture(format!("cutmap{mat}_{slot}"), to_color_image(&adjusted(&img, &s.adj), 512), egui::TextureOptions::LINEAR))
            })
            .clone()
    }
}

const ACCENT: Color32 = Color32::from_rgb(0x47, 0x72, 0xb3);

/// A rounded strip holding a few toolbar buttons.
fn pill(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().fill(Color32::from_gray(40)).corner_radius(8).inner_margin(egui::Margin::symmetric(4, 3)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            add(ui);
        });
    });
}

fn icon(text: &str, on: bool) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).size(15.5).color(if on { Color32::WHITE } else { Color32::from_gray(215) }))
        .fill(if on { ACCENT } else { Color32::TRANSPARENT })
        .corner_radius(6)
        .min_size(Vec2::new(30.0, 28.0))
}

fn tool_btn(ui: &mut egui::Ui, ed: &mut MeshEdit, t: Tool, text: &str, tip: &str) {
    if ui.add(icon(text, ed.tool == t)).on_hover_text(tip).clicked() {
        ed.tool = t;
        ed.drag = None;
    }
}

fn act_btn(ui: &mut egui::Ui, text: &str, tip: &str) -> bool {
    ui.add(icon(text, false)).on_hover_text(tip).clicked()
}

fn arrow_head(p: &egui::Painter, from: Pos2, to: Pos2, size: f32, col: Color32) {
    let d = (to - from).normalized();
    let n = Vec2::new(-d.y, d.x);
    p.add(egui::Shape::convex_polygon(vec![to + d * size * 0.4, to - d * size * 0.9 + n * size * 0.6, to - d * size * 0.9 - n * size * 0.6], col, Stroke::NONE));
}

/// The scene: the working cutout flat (Angle shown by the pivot's arrow instead), its points,
/// anchors, painted spots and pivot.
fn draw_scene(p: &egui::Painter, v: &View, ed: &MeshEdit, hover: Option<Pos2>) {
    let s = &ed.scene;
    // An empty scene shows no mesh until it has three points.
    let (pts, tris) = if s.lods[0].len() >= 3 { cutout::build(&unturned(s, 0), 0) } else { (vec![], vec![]) };
    let mut fill = egui::Mesh::default();
    for t in &tris {
        let base = fill.vertices.len() as u32;
        for &k in t {
            fill.colored_vertex(v.to(pts[k]), Color32::from_rgba_unmultiplied(40, 200, 225, 30));
        }
        fill.add_triangle(base, base + 1, base + 2);
    }
    p.add(egui::Shape::mesh(fill));
    let edge = Stroke::new(1.1, Color32::from_rgba_unmultiplied(70, 220, 240, 210));
    for t in &tris {
        let q: Vec<Pos2> = t.iter().map(|&k| v.to(pts[k])).collect();
        for k in 0..3 {
            p.line_segment([q[k], q[(k + 1) % 3]], edge);
        }
    }
    for mk in &s.mask[0] {
        p.circle_filled(v.to([mk[0], mk[1]]), (mk[2] * v.card().width()).max(2.0), Color32::from_rgba_unmultiplied(235, 60, 60, 70));
    }
    if ed.mirror {
        let x = v.to([0.5, 0.0]).x;
        let c = v.card();
        let mut y = c.top();
        while y < c.bottom() {
            p.line_segment([Pos2::new(x, y), Pos2::new(x, (y + 6.0).min(c.bottom()))], Stroke::new(1.0, Color32::from_white_alpha(120)));
            y += 11.0;
        }
    }
    let points_on = ed.tool == Tool::Point;
    for (i, q) in s.lods[0].iter().enumerate() {
        let sp = v.to(*q);
        let sel = ed.selected.contains(&i);
        let hot = points_on && hover.is_some_and(|h| h.distance(sp) < 8.0);
        let r = if sel || hot { 5.5 } else { 4.2 };
        p.circle_filled(sp + Vec2::new(0.0, 1.0), r + 1.0, Color32::from_black_alpha(90));
        p.circle_filled(sp, r, if sel { Color32::from_rgb(255, 165, 40) } else if points_on { Color32::WHITE } else { Color32::from_gray(200) });
        p.circle_stroke(sp, r, Stroke::new(1.2, Color32::from_gray(30)));
    }
    let green = Color32::from_rgb(70, 215, 110);
    for a in &s.anchors {
        let sp = v.to(a.pos);
        let e = v.to([a.pos[0] + a.dir[0], a.pos[1] + a.dir[1]]);
        p.line_segment([sp, e], Stroke::new(2.0, green));
        if sp.distance(e) > 4.0 {
            arrow_head(p, sp, e, 9.0, green);
        }
        p.circle_filled(sp, 5.5, green);
        p.circle_stroke(sp, 5.5, Stroke::new(1.2, Color32::from_gray(25)));
        p.text(sp + Vec2::new(8.0, -8.0), egui::Align2::LEFT_BOTTOM, a.id.to_string(), egui::FontId::proportional(11.5), green);
    }
    // Pivot and its arrow: where the leaf attaches and which way it grows.
    let orange = Color32::from_rgb(250, 160, 50);
    let piv = v.to(s.pivot);
    let tip = v.to(ed.arrow_tip());
    p.line_segment([piv, tip], Stroke::new(2.2, orange));
    arrow_head(p, piv, tip, 13.0, orange);
    p.circle_filled(piv, 7.5, Color32::from_gray(25));
    p.circle_stroke(piv, 7.5, Stroke::new(2.0, orange));
    p.line_segment([piv - Vec2::new(4.0, 0.0), piv + Vec2::new(4.0, 0.0)], Stroke::new(1.5, orange));
    p.line_segment([piv - Vec2::new(0.0, 4.0), piv + Vec2::new(0.0, 4.0)], Stroke::new(1.5, orange));
}

fn point_in_poly(p: Pos2, poly: &[Pos2]) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// The Cutout Editor. Returns the edited mesh when Done is pressed.
pub fn edit_mesh_window(ctx: &egui::Context, st: &mut EditorState, doc: &Doc, pics: &Pics) -> Option<(usize, MeshAsset)> {
    let ed = st.edit_mesh.as_mut()?;
    let mut done = None;
    let mut close = false;
    let mut message = st.note.clone();
    let screen = ctx.content_rect();
    egui::Window::new(format!("Cutout Editor — {}", ed.mesh.name))
        .id(egui::Id::unique("cutout_editor"))
        .collapsible(false)
        .resizable(true)
        .default_size([screen.width().min(1240.0) * 0.92, screen.height().min(860.0) * 0.88])
        .default_pos(screen.center() - Vec2::new(screen.width().min(1240.0) * 0.46, screen.height().min(860.0) * 0.44))
        .show(ctx, |ui| {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
                if let Some(m) = ed.history.pop() {
                    ed.scene = m;
                    ed.selected.clear();
                }
            }
            // ------------------------------------------------ toolbar
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let glyph = |m: SelMode| match m {
                    SelMode::Box => "▭",
                    SelMode::Circle => "◯",
                    SelMode::Lasso => "➰",
                };
                let r = egui::ComboBox::from_id_salt("cut_selmode")
                    .selected_text(RichText::new(glyph(ed.sel_mode)).size(15.5))
                    .width(34.0)
                    .show_ui(ui, |ui| {
                        for (m, name) in [(SelMode::Box, "Box"), (SelMode::Circle, "Circle"), (SelMode::Lasso, "Lasso")] {
                            ui.selectable_value(&mut ed.sel_mode, m, format!("{}  {name} select", glyph(m)));
                        }
                    });
                r.response.on_hover_text("Selection shape: dragging empty space with the Point tool selects the points inside it");
                pill(ui, |ui| {
                    tool_btn(ui, ed, Tool::Point, "➕", "Point tool: click to add a point, double-click a point to delete it.\nDrag points, the pivot or its orange arrow to move them; drag empty space to select.");
                    if act_btn(ui, "↺", "Reset card: go back to this LOD's default shape (High 8 points, Medium 6, Low a diamond)") {
                        ed.save_undo();
                        ed.scene.lods[0] = [MeshAsset::card8(), MeshAsset::card6(), MeshAsset::diamond()][ed.lod.min(2)].clone();
                        ed.scene.mask[0].clear();
                        ed.scene.colors.clear();
                        ed.selected.clear();
                    }
                    if ui.add(icon("⇋", ed.mirror)).on_hover_text("Mirror: make the left side match the right; new points are added on both sides").clicked() {
                        ed.mirror = !ed.mirror;
                        if ed.mirror {
                            ed.save_undo();
                            let m = cutout::mirror(ed.pts());
                            *ed.pts() = m;
                            ed.selected.clear();
                        }
                    }
                });
                pill(ui, |ui| {
                    tool_btn(ui, ed, Tool::Anchor, "📌", "Anchor tool: click or drag to place an anchor (drag sets its direction).\nDrag an anchor or its arrow to edit it; double-click it to delete. Right-click for its ID.");
                    if act_btn(ui, "🗑", "Delete every anchor") {
                        ed.save_undo();
                        ed.scene.anchors.clear();
                    }
                });
                pill(ui, |ui| {
                    tool_btn(ui, ed, Tool::Brush, "🖌", "Brush: paint parts of the card see-through.\nCtrl + paint, or double-click and hold, erases.");
                    ui.menu_button(RichText::new("⚙").size(15.5), |ui| {
                        ui.add(egui::Slider::new(&mut ed.brush_size, 0.01..=0.4).text("Size"));
                        ui.horizontal(|ui| {
                            ui.label("Size is");
                            ui.selectable_value(&mut ed.brush_screen, false, "on the card").on_hover_text("The brush covers the same part of the card at any zoom");
                            ui.selectable_value(&mut ed.brush_screen, true, "on screen").on_hover_text("The brush keeps its size on screen as you zoom");
                        });
                    })
                    .response
                    .on_hover_text("Brush settings");
                    if act_btn(ui, "🧹", "Clear all paint") {
                        ed.save_undo();
                        ed.scene.mask[0].clear();
                    }
                });
                pill(ui, |ui| {
                    if act_btn(ui, "✨", "Generate: trace the cutout from the material's opacity") {
                        let traced = ed.scene.material.and_then(|i| doc.materials.get(i)).and_then(load_picture).and_then(|img| {
                            let a: Vec<u8> = img.pixels().map(|p| p[3]).collect();
                            cutout::from_alpha(&a, img.width() as usize, img.height() as usize, ed.density)
                        });
                        ed.save_undo();
                        match traced {
                            Some((pts, mask)) => {
                                ed.scene.lods[0] = pts;
                                ed.scene.mask[0] = mask;
                                message = None;
                            }
                            None => {
                                ed.scene.lods[0] = cutout::generate(0, ed.density);
                                ed.scene.mask[0].clear();
                                message = Some("No picture with opacity to trace, so Generate made a leaf outline.".into());
                            }
                        }
                        ed.scene.colors.clear();
                        ed.selected.clear();
                    }
                    ui.menu_button(RichText::new("⚙").size(15.5), |ui| {
                        ui.add(egui::Slider::new(&mut ed.density, 6..=120).text("Points")).on_hover_text("How many points Generate places around the edge");
                    })
                    .response
                    .on_hover_text("Generate settings");
                });
                pill(ui, |ui| {
                    ui.menu_button(RichText::new("🎨").size(15.5), |ui| {
                        ui.set_min_width(250.0);
                        ui.label(RichText::new("Material").strong());
                        let names: Vec<String> = doc.materials.iter().map(|m| m.name.clone()).collect();
                        let mut opts = vec!["None"];
                        opts.extend(names.iter().map(String::as_str));
                        let mut i = ed.scene.material.map_or(0, |x| x + 1);
                        if combo(ui, "cutmat", &mut i, &opts, 240.0) {
                            ed.scene.material = i.checked_sub(1);
                            ed.dirty = true;
                        }
                        if let Some(mi) = ed.scene.material.filter(|&i| i < doc.materials.len()) {
                            let slots: Vec<usize> = MAP_SHOWN.iter().copied().filter(|&s| doc.materials[mi].maps.get(s).is_some_and(|m| m.has_picture())).collect();
                            if slots.is_empty() {
                                ui.label(RichText::new("This material has no pictures.").size(11.5).weak());
                            }
                            let cell = 74.0;
                            for row in slots.chunks(3) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    for &s in row {
                                        let tex = ed.map_tex(ui.ctx(), doc, pics, mi, s);
                                        let (r, resp) = ui.allocate_exact_size(Vec2::new(cell, cell + 14.0), Sense::click());
                                        let img = Rect::from_min_size(r.min, Vec2::splat(cell));
                                        let pp = ui.painter_at(r);
                                        checker(&pp, img, 6.0);
                                        if let Some(t) = tex {
                                            pp.image(t.id(), img, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                                        }
                                        let on = ed.bg_map == s;
                                        pp.rect_stroke(img, CornerRadius::same(4), Stroke::new(if on { 2.0 } else { 1.0 }, if on { ACCENT } else { Color32::from_gray(70) }), egui::StrokeKind::Inside);
                                        pp.text(Pos2::new(img.center().x, img.bottom() + 1.0), egui::Align2::CENTER_TOP, map_label(s), egui::FontId::proportional(10.5), if on { Color32::WHITE } else { Color32::from_gray(170) });
                                        if resp.on_hover_text(format!("Show the {} map behind the mesh", map_label(s))).clicked() {
                                            ed.bg_map = s;
                                        }
                                    }
                                });
                            }
                        }
                    })
                    .response
                    .on_hover_text("Material: pick it and which of its maps shows behind the mesh");
                    ui.menu_button(RichText::new("▦").size(15.5), |ui| {
                        ui.set_min_width(240.0);
                        ui.label(RichText::new("Mesh options").strong());
                        let mut ch = false;
                        lrow(ui, 92.0, "Tessellation", |ui| {
                            ch |= combo(ui, "tess", &mut ed.scene.tess_style, &["Triangular", "Split", "Grid"], 120.0);
                        });
                        lrow(ui, 92.0, "Amount", |ui| {
                            ch |= ui.add(egui::Slider::new(&mut ed.scene.tess_amount, 0.0..=1.0)).on_hover_text("How many times each triangle is split").changed();
                        });
                        lrow(ui, 92.0, "Refine edges", |ui| {
                            ch |= ui.add(egui::Slider::new(&mut ed.scene.refine, 0.0..=1.0)).on_hover_text("Extra points along the outline").changed();
                        });
                        lrow(ui, 92.0, "Double-sided", |ui| {
                            ch |= ui.checkbox(&mut ed.scene.double_sided, "").on_hover_text("Draw the back of the leaf too").changed();
                        });
                        ed.dirty |= ch;
                    })
                    .response
                    .on_hover_text("Mesh options: tessellation, edge refinement and double-sided");
                });
                pill(ui, |ui| {
                    if act_btn(ui, "📂", "Import a mesh (OBJ) into the scene") {
                        if let Some(path) = obj_dialog().pick_file() {
                            match import_obj(&path) {
                                Ok(pts) => {
                                    ed.save_undo();
                                    ed.scene.lods[0] = pts;
                                    ed.scene.mask[0].clear();
                                    ed.scene.colors.clear();
                                    ed.selected.clear();
                                    message = Some(format!("Imported {}.", path.display()));
                                }
                                Err(e) => message = Some(format!("Couldn't import the mesh: {e}")),
                            }
                        }
                    }
                    if act_btn(ui, "💾", "Export the scene's cutout as a mesh (OBJ)") {
                        if let Some(path) = obj_dialog().set_file_name(format!("{}.obj", ed.mesh.name)).save_file() {
                            message = Some(match export_obj(&ed.scene, 0, &path) {
                                Ok(()) => format!("Saved {}.", path.display()),
                                Err(e) => format!("Couldn't save the mesh: {e}"),
                            });
                        }
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if act_btn(ui, "⊡", "Fit the card in the view") {
                        ed.zoom = 1.0;
                        ed.pan = Vec2::ZERO;
                    }
                });
            });
            if let Some(msg) = &message {
                ui.label(RichText::new(msg).size(11.5).color(Color32::from_rgb(240, 190, 90)));
            }
            ui.add_space(4.0);
            let avail = ui.available_size();
            let lod_w = 176.0;
            let canvas = Vec2::new((avail.x - lod_w - 14.0).max(240.0), (avail.y - 40.0).max(240.0));
            ui.horizontal_top(|ui| {
                // ------------------------------------------------ scene
                let (r, resp) = ui.allocate_exact_size(canvas, Sense::click_and_drag());
                let p = ui.painter_at(r);
                p.rect_filled(r, CornerRadius::same(8), Color32::from_gray(30));
                if resp.hovered() {
                    let z = ui.input(|i| i.smooth_scroll_delta.y);
                    if z != 0.0 {
                        ed.zoom = (ed.zoom * (z * 0.002).exp()).clamp(0.2, 8.0);
                    }
                }
                if resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Secondary) {
                    ed.pan += resp.drag_delta();
                }
                let v = View { r, zoom: ed.zoom, pan: ed.pan };
                let card = v.card();
                checker(&p, card, 10.0);
                let bg = ed.scene.material.filter(|&i| i < doc.materials.len()).and_then(|mi| {
                    let slot = ed.bg_map;
                    ed.map_tex(ui.ctx(), doc, pics, mi, slot).or_else(|| ed.map_tex(ui.ctx(), doc, pics, mi, 0))
                });
                match &bg {
                    Some(t) => {
                        p.image(t.id(), card, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        p.rect_filled(card, CornerRadius::same(4), Color32::from_rgba_unmultiplied(214, 250, 252, 40));
                    }
                }
                p.rect_stroke(card, CornerRadius::same(2), Stroke::new(1.0, Color32::from_white_alpha(60)), egui::StrokeKind::Outside);
                let hover = resp.hover_pos();
                draw_scene(&p, &v, ed, hover);
                p.text(r.left_top() + Vec2::new(10.0, 8.0), egui::Align2::LEFT_TOP, if ed.dirty { format!("Scene · changed since {}", LOD_NAMES[ed.lod]) } else { format!("Scene · same as {}", LOD_NAMES[ed.lod]) }, egui::FontId::proportional(11.5), if ed.dirty { Color32::from_rgb(240, 190, 90) } else { Color32::from_gray(150) });

                // ------------------------------------------------ interaction
                let (shift, ctrl, now) = ui.input(|i| (i.modifiers.shift, i.modifiers.command, i.time));
                let origin = ui.input(|i| i.pointer.press_origin());
                let hit_pt = |ed: &MeshEdit, pos: Pos2| ed.scene.lods[0].iter().position(|w| v.to(*w).distance(pos) < 8.0);
                let hit_anchor = |ed: &MeshEdit, pos: Pos2| ed.scene.anchors.iter().position(|a| v.to(a.pos).distance(pos) < 8.0);
                let hit_dir = |ed: &MeshEdit, pos: Pos2| ed.scene.anchors.iter().position(|a| v.to([a.pos[0] + a.dir[0], a.pos[1] + a.dir[1]]).distance(pos) < 9.0);
                let on_pivot = |ed: &MeshEdit, pos: Pos2| v.to(ed.scene.pivot).distance(pos) < 10.0 || v.to(ed.arrow_tip()).distance(pos) < 10.0;
                let pos = resp.interact_pointer_pos();
                match ed.tool {
                    Tool::Point => {
                        if let Some(at) = pos.filter(|_| resp.double_clicked()) {
                            // The first click added or picked a point; a double-click on a point deletes it.
                            if let Some(i) = hit_pt(ed, at) {
                                ed.save_undo();
                                ed.remove_point(i);
                            }
                        } else if let Some(at) = pos.filter(|_| resp.clicked()) {
                            match hit_pt(ed, at) {
                                Some(i) if shift => {
                                    if let Some(k) = ed.selected.iter().position(|&s| s == i) {
                                        ed.selected.remove(k);
                                    } else {
                                        ed.selected.push(i);
                                    }
                                }
                                Some(i) => ed.selected = vec![i],
                                None if !on_pivot(ed, at) => {
                                    ed.save_undo();
                                    let q = v.from(at);
                                    ed.pts().push(q);
                                    if ed.mirror && (q[0] - 0.5).abs() > 0.02 {
                                        ed.pts().push([1.0 - q[0], q[1]]);
                                    }
                                    if !ed.scene.colors.is_empty() {
                                        let n = ed.scene.lods[0].len();
                                        ed.scene.colors.resize(n, [255; 3]);
                                    }
                                    // Keep the new point picked so a quick second click deletes it again.
                                    ed.selected = vec![ed.scene.lods[0].len() - 1];
                                }
                                None => {}
                            }
                        }
                        if resp.drag_started_by(egui::PointerButton::Primary) {
                            if let Some(o) = origin {
                                ed.drag = Some(if v.to(ed.arrow_tip()).distance(o) < 10.0 {
                                    ed.save_undo();
                                    Drag::Angle
                                } else if v.to(ed.scene.pivot).distance(o) < 10.0 {
                                    ed.save_undo();
                                    Drag::Pivot
                                } else if let Some(i) = hit_pt(ed, o) {
                                    if !ed.selected.contains(&i) {
                                        if !shift {
                                            ed.selected.clear();
                                        }
                                        ed.selected.push(i);
                                    }
                                    ed.save_undo();
                                    Drag::Points(v.from(o))
                                } else {
                                    ed.sel_path = vec![o];
                                    Drag::Select
                                });
                            }
                        }
                    }
                    Tool::Anchor => {
                        if let Some(at) = pos.filter(|_| resp.double_clicked()) {
                            if let Some(i) = hit_anchor(ed, at) {
                                ed.save_undo();
                                ed.scene.anchors.remove(i);
                            }
                        } else if let Some(at) = pos.filter(|_| resp.clicked()) {
                            if hit_anchor(ed, at).is_none() && hit_dir(ed, at).is_none() {
                                ed.save_undo();
                                ed.scene.anchors.push(Anchor { pos: v.from(at), dir: [0.0, 0.15], id: 1 });
                            }
                        }
                        if resp.drag_started_by(egui::PointerButton::Primary) {
                            if let Some(o) = origin {
                                ed.save_undo();
                                ed.drag = Some(match (hit_dir(ed, o), hit_anchor(ed, o)) {
                                    (Some(i), _) => Drag::AnchorDir(i),
                                    (None, Some(i)) => Drag::Anchor(i),
                                    _ => {
                                        ed.scene.anchors.push(Anchor { pos: v.from(o), dir: [0.0, 0.0], id: 1 });
                                        Drag::AnchorDir(ed.scene.anchors.len() - 1)
                                    }
                                });
                            }
                        }
                    }
                    Tool::Brush => {
                        // A second press soon after the first starts an erasing stroke.
                        if resp.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
                            ed.erase_stroke = now - ed.last_press < 0.35;
                            ed.last_press = now;
                            ed.save_undo();
                            ed.drag = Some(Drag::Paint);
                        }
                    }
                }
                // While the button is held.
                let held = resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down());
                if let Some(at) = pos.or(hover).filter(|_| held || resp.clicked()) {
                    let q = v.from(at);
                    match ed.drag {
                        Some(Drag::Pivot) => ed.scene.pivot = q,
                        Some(Drag::Angle) => {
                            let (dx, dy) = (q[0] - ed.scene.pivot[0], q[1] - ed.scene.pivot[1]);
                            if dx.hypot(dy) > 1e-4 {
                                ed.scene.angle = (-dx).atan2(dy).to_degrees().round();
                            }
                        }
                        Some(Drag::Points(prev)) => {
                            let d = [q[0] - prev[0], q[1] - prev[1]];
                            for &i in &ed.selected {
                                if let Some(w) = ed.scene.lods[0].get_mut(i) {
                                    w[0] += d[0];
                                    w[1] += d[1];
                                }
                            }
                            ed.drag = Some(Drag::Points(q));
                        }
                        Some(Drag::Anchor(i)) => {
                            if let Some(a) = ed.scene.anchors.get_mut(i) {
                                a.pos = q;
                            }
                        }
                        Some(Drag::AnchorDir(i)) => {
                            if let Some(a) = ed.scene.anchors.get_mut(i) {
                                a.dir = [q[0] - a.pos[0], q[1] - a.pos[1]];
                            }
                        }
                        Some(Drag::Select) => {
                            if ed.sel_path.last().is_none_or(|l| l.distance(at) > 3.0) {
                                ed.sel_path.push(at);
                            }
                        }
                        Some(Drag::Paint) => {
                            let br = ed.brush_r();
                            if ed.erase_stroke || ctrl {
                                ed.scene.mask[0].retain(|m| (m[0] - q[0]).hypot(m[1] - q[1]) > br);
                            } else if ed.scene.mask[0].last().is_none_or(|m| (m[0] - q[0]).hypot(m[1] - q[1]) > br * 0.3) {
                                ed.scene.mask[0].push([q[0], q[1], br]);
                            }
                        }
                        None => {}
                    }
                }
                // Selection shape while dragging, and the brush outline.
                if ed.drag == Some(Drag::Select) && !ed.sel_path.is_empty() {
                    let (a, b) = (ed.sel_path[0], *ed.sel_path.last().unwrap());
                    let fill = Color32::from_rgba_unmultiplied(255, 200, 80, 22);
                    let s = Stroke::new(1.2, Color32::from_rgb(255, 200, 80));
                    match ed.sel_mode {
                        SelMode::Box => {
                            p.rect(Rect::from_two_pos(a, b), CornerRadius::same(2), fill, s, egui::StrokeKind::Inside);
                        }
                        SelMode::Circle => {
                            p.circle(a, a.distance(b), fill, s);
                        }
                        SelMode::Lasso => {
                            let mut pts = ed.sel_path.clone();
                            pts.push(a);
                            p.add(egui::Shape::line(pts, s));
                        }
                    }
                }
                if ed.tool == Tool::Brush {
                    if let Some(h) = hover {
                        let erase = ctrl || (ed.erase_stroke && ed.drag == Some(Drag::Paint));
                        p.circle_stroke(h, ed.brush_r() * v.card().width(), Stroke::new(1.5, if erase { Color32::from_rgb(255, 110, 110) } else { Color32::WHITE }));
                    }
                }
                // Letting go ends the drag (and a selection drag picks its points).
                if ed.drag.is_some() && !ui.input(|i| i.pointer.primary_down()) {
                    if ed.drag == Some(Drag::Select) && ed.sel_path.len() > 1 {
                        let (a, b) = (ed.sel_path[0], *ed.sel_path.last().unwrap());
                        let inside = |s: Pos2| match ed.sel_mode {
                            SelMode::Box => Rect::from_two_pos(a, b).contains(s),
                            SelMode::Circle => s.distance(a) <= a.distance(b),
                            SelMode::Lasso => point_in_poly(s, &ed.sel_path),
                        };
                        let hits: Vec<usize> = ed.scene.lods[0].iter().enumerate().filter(|(_, w)| inside(v.to(**w))).map(|(i, _)| i).collect();
                        if !shift {
                            ed.selected.clear();
                        }
                        for i in hits {
                            if !ed.selected.contains(&i) {
                                ed.selected.push(i);
                            }
                        }
                    }
                    ed.drag = None;
                    ed.sel_path.clear();
                }
                let del = ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
                if del && ed.tool == Tool::Point && !ed.selected.is_empty() && !ui.ctx().egui_wants_keyboard_input() {
                    ed.save_undo();
                    let mut sel = ed.selected.clone();
                    sel.sort_unstable_by(|a, b| b.cmp(a));
                    for i in sel {
                        if i < ed.scene.lods[0].len() {
                            ed.scene.lods[0].remove(i);
                            if i < ed.scene.colors.len() {
                                ed.scene.colors.remove(i);
                            }
                        }
                    }
                    ed.selected.clear();
                }
                if let Some(h) = hover {
                    let busy = matches!(ed.drag, Some(Drag::Points(_) | Drag::Pivot | Drag::Angle | Drag::Anchor(_) | Drag::AnchorDir(_)));
                    let over = match ed.tool {
                        Tool::Point => hit_pt(ed, h).is_some() || on_pivot(ed, h),
                        Tool::Anchor => hit_anchor(ed, h).is_some() || hit_dir(ed, h).is_some(),
                        Tool::Brush => false,
                    };
                    if busy {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    } else if over {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    } else {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                    }
                }
                let anchor_hit = hover.and_then(|h| hit_anchor(ed, h));
                resp.context_menu(|ui| {
                    if let Some(i) = anchor_hit {
                        ui.label("Anchor ID");
                        if ui.add(egui::DragValue::new(&mut ed.scene.anchors[i].id).range(0..=99)).changed() {
                            ed.dirty = true;
                        }
                        ui.separator();
                    }
                    if ui.button("Select all points").clicked() {
                        ed.selected = (0..ed.scene.lods[0].len()).collect();
                        ui.close();
                    }
                    if ui.button("Clear selection").clicked() {
                        ed.selected.clear();
                        ui.close();
                    }
                });
                // ------------------------------------------------ LOD slots
                ui.vertical(|ui| {
                    ui.set_width(lod_w);
                    ui.label(RichText::new("LOD slots").size(12.5).color(Color32::from_gray(190))).on_hover_text("Each slot keeps its own points, paint, anchors, pivot and mesh options");
                    for (l, name) in LOD_NAMES.iter().enumerate() {
                        let linked = ed.lod == l;
                        egui::Frame::new()
                            .fill(Color32::from_gray(if linked { 50 } else { 40 }))
                            .stroke(Stroke::new(if linked { 1.5 } else { 1.0 }, if linked { ACCENT } else { Color32::from_gray(58) }))
                            .corner_radius(8)
                            .inner_margin(6)
                            .show(ui, |ui| {
                                ui.label(RichText::new(*name).strong().color(if linked { Color32::WHITE } else { Color32::from_gray(200) }));
                                let side = lod_w - 12.0;
                                let (pr, presp) = ui.allocate_exact_size(Vec2::new(side, side - 34.0), Sense::hover());
                                let pp = ui.painter_at(pr);
                                pp.rect_filled(pr, CornerRadius::same(5), Color32::from_gray(28));
                                let pv = View { r: pr, zoom: 0.9, pan: Vec2::new(10.0, 0.0) };
                                if ed.mesh.lods[l].len() >= 3 {
                                    checker(&pp, pv.card(), 6.0);
                                    if let Some(t) = &bg {
                                        pp.image(t.id(), pv.card(), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                                    }
                                    draw_cutout(&pp, &pv, &ed.mesh, l, Color32::from_rgb(70, 220, 240), false, &[]);
                                } else {
                                    pp.text(pv.card().center(), egui::Align2::CENTER_CENTER, "Empty", egui::FontId::proportional(12.0), Color32::from_gray(130));
                                }
                                presp.on_hover_text(format!("The saved {name} cutout. It only changes when you send the scene to it."));
                                // ➡ top-left: scene to slot; ⬅ bottom-left: slot to scene.
                                let b = Vec2::new(26.0, 22.0);
                                let to_slot = Rect::from_min_size(pr.left_top() + Vec2::new(3.0, 3.0), b);
                                let to_scene = Rect::from_min_size(pr.left_bottom() + Vec2::new(3.0, -3.0 - b.y), b);
                                if ui.put(to_slot, egui::Button::new(RichText::new("➡").size(13.0)).corner_radius(5)).on_hover_text(format!("Put the scene (points, paint, anchors, pivot and mesh options) into the {name} slot")).clicked() {
                                    ed.store(l);
                                }
                                if ui.put(to_scene, egui::Button::new(RichText::new("⬅").size(13.0)).corner_radius(5)).on_hover_text(format!("Bring the {name} slot into the scene")).clicked() {
                                    if ed.dirty {
                                        ed.ask = Some(Ask::Load(l));
                                    } else {
                                        ed.load(l);
                                    }
                                }
                                let (vp, vt) = cutout::build(&ed.mesh, l);
                                ui.label(RichText::new(format!("{} points · {} triangles", vp.len(), vt.len())).size(11.0).color(Color32::from_gray(165)));
                            });
                        ui.add_space(5.0);
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let (vp, vt) = cutout::build(&ed.scene, 0);
                let hint = match ed.tool {
                    Tool::Point => "Click: add a point · double-click a point: delete · drag: move or select",
                    Tool::Anchor => "Click or drag: place · drag an anchor: edit · double-click: delete",
                    Tool::Brush => "Drag: paint · Ctrl + drag, or double-click and hold: erase",
                };
                ui.label(RichText::new(format!("{} points · {} triangles · {} selected · turned {:.0}°", vp.len(), vt.len(), ed.selected.len(), ed.scene.angle)).color(Color32::from_gray(175)));
                ui.label(RichText::new(format!("   {hint} · wheel: zoom · right-drag: pan · Ctrl+Z: undo")).size(11.0).color(Color32::from_gray(130)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("Done").size(14.0).color(Color32::WHITE)).fill(ACCENT).corner_radius(6).min_size(Vec2::new(110.0, 28.0))).on_hover_text("Keep the LOD slots and close").clicked() {
                        if ed.dirty {
                            ed.ask = Some(Ask::Close(true));
                        } else {
                            done = Some((ed.index, ed.mesh.clone()));
                            close = true;
                        }
                    }
                    if ui.add(egui::Button::new(RichText::new("Cancel").size(14.0)).corner_radius(6).min_size(Vec2::new(110.0, 28.0))).on_hover_text("Close without keeping anything").clicked() {
                        if ed.dirty {
                            ed.ask = Some(Ask::Close(false));
                        } else {
                            close = true;
                        }
                    }
                });
            });
        });
    // Unsaved scene changes: ask before they're lost.
    if let Some(ask) = ed.ask {
        let text = match ask {
            Ask::Load(l) => format!("The scene has changes that aren't in any LOD slot. Bring the {} slot in and overwrite them?", LOD_NAMES[l]),
            Ask::Close(_) => "The scene has changes that aren't in any LOD slot. Close the Cutout Editor and lose them?".to_string(),
        };
        egui::Window::new("Unsaved cutout changes").id(egui::Id::unique("cutout_ask")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).order(egui::Order::Foreground).show(ctx, |ui| {
            ui.label(text);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let slot = ed.lod;
                if let Ask::Close(keep) = ask {
                    if keep && ui.button(format!("Save to {} and close", LOD_NAMES[slot])).on_hover_text("Put the scene into its slot first").clicked() {
                        ed.store(slot);
                        done = Some((ed.index, ed.mesh.clone()));
                        close = true;
                    }
                }
                let yes = if matches!(ask, Ask::Load(_)) { "Overwrite" } else { "Close without saving" };
                if ui.button(yes).clicked() {
                    match ask {
                        Ask::Load(l) => ed.load(l),
                        Ask::Close(keep) => {
                            if keep {
                                done = Some((ed.index, ed.mesh.clone()));
                            }
                            close = true;
                        }
                    }
                    ed.ask = None;
                }
                if ui.button("Keep editing").clicked() {
                    ed.ask = None;
                }
            });
        });
    }
    st.note = if close { None } else { message };
    if close {
        st.edit_mesh = None;
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn map_adjustments_change_pixels() {
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([200, 100, 50, 255]));
        assert_eq!(adjusted(&img, &MapAdjust::default()), img, "no adjustment, no change");
        let inv = adjusted(&img, &MapAdjust { invert: true, ..MapAdjust::default() });
        assert_eq!(inv.get_pixel(0, 0).0, [55, 155, 205, 255]);
        let red = adjusted(&img, &MapAdjust { channel: 1, ..MapAdjust::default() });
        assert_eq!(red.get_pixel(0, 0).0, [200, 200, 200, 255], "one channel shows as grey");
        let a = MapAdjust { recolor: [[255, 0, 0], [255, 0, 0]], ..MapAdjust::default() };
        assert_eq!(adjusted(&img, &a).get_pixel(0, 0).0, [255, 0, 0, 255], "recolor paints the gradient");
    }

    #[test]
    fn rect_areas_stay_rectangles() {
        let mut pts = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        drag_rect_corner(&mut pts, 2, [0.5, 0.6]);
        assert_eq!(pts, [[0.0, 0.0], [0.5, 0.0], [0.5, 0.6], [0.0, 0.6]]);
    }

    #[test]
    fn obj_export_import_round_trips() {
        let m = MeshAsset::leaf_card();
        let path = std::env::temp_dir().join("treecraft_cutout_test.obj");
        export_obj(&m, 0, &path).unwrap();
        let pts = import_obj(&path).unwrap();
        assert_eq!(pts.len(), cutout::build(&m, 0).0.len(), "every point comes back");
        assert!(pts.iter().all(|p| m.lods[0].iter().any(|q| (q[0] - p[0]).abs() < 1e-4 && (q[1] - p[1]).abs() < 1e-4)), "at the same place on the card");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn slots_keep_their_own_settings() {
        let mut ed = MeshEdit::new(0, MeshAsset::leaf_card());
        ed.scene.tess_amount = 1.0;
        ed.store(1);
        ed.scene.tess_amount = 0.0;
        ed.scene.lods[0].push([0.3, 0.9]);
        assert_eq!(ed.mesh.cfg(1).tess_amount, 1.0, "a stored slot doesn't follow later scene edits");
        assert_ne!(ed.mesh.lods[1].len(), ed.scene.lods[0].len());
        ed.load(1);
        assert_eq!(ed.scene.tess_amount, 1.0, "bringing a slot back restores its settings");
    }

    #[test]
    fn view_round_trips() {
        let v = View { r: Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0)), zoom: 1.5, pan: Vec2::new(10.0, -5.0) };
        let q = [0.3, 0.7];
        let back = v.from(v.to(q));
        assert!((back[0] - q[0]).abs() < 1e-4 && (back[1] - q[1]).abs() < 1e-4);
    }
}
