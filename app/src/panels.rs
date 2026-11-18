//! Right-hand editors: Materials (with picture slots), Material Sets, Meshes, Displacements, and
//! the Cutout Editor dialog. Also loads material pictures for the views and the 3D atlas.

use crate::cutout;
use crate::math::hash;
use crate::model::{Anchor, Doc, Material, MeshAsset, MAP_IS_COLOR, MAP_NAMES};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Vec2};
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
    pub sets: Vec<String>,
    pub edit_mesh: Option<MeshEdit>,
    pub message: Option<String>,
}

// ---------------------------------------------------------------- pictures

/// Material pictures as textures, plus one atlas the 3D view samples leaves and fronds from.
#[derive(Default)]
pub struct Pics {
    sig: u64,
    pub color: Vec<Option<egui::TextureHandle>>,
    pub alpha: Vec<Option<egui::TextureHandle>>,
    /// The small preview picture a SpeedTree file carries for each material.
    pub preview: Vec<Option<egui::TextureHandle>>,
    pub atlas: Option<egui::TextureHandle>,
    /// Per material: uv rectangle [u0, v0, u1, v1] in the atlas.
    pub cells: Vec<Option<[f32; 4]>>,
    /// A white spot in the atlas for untextured triangles.
    pub white: [f32; 2],
}

fn load_picture(m: &Material) -> Option<image::RgbaImage> {
    let mut img = image::open(m.maps[0].file.as_ref().filter(|_| m.maps[0].on)?).ok()?.to_rgba8();
    if img.width() > 1024 || img.height() > 1024 {
        img = image::imageops::resize(&img, 1024, 1024, image::imageops::FilterType::Triangle);
    }
    if let Some(op) = m.maps[1].file.as_ref().filter(|_| m.maps[1].on).and_then(|f| image::open(f).ok()) {
        let op = image::imageops::resize(&op.to_luma8(), img.width(), img.height(), image::imageops::FilterType::Triangle);
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
            m.maps[0].file.hash(&mut h);
            m.maps[1].file.hash(&mut h);
            m.maps[1].on.hash(&mut h);
            m.preview_png.as_ref().map(|b| b.len()).hash(&mut h);
        }
        mats.len().hash(&mut h);
        let sig = h.finish();
        if sig == self.sig && self.atlas.is_some() {
            return;
        }
        self.sig = sig;
        let imgs: Vec<Option<image::RgbaImage>> = mats.iter().map(load_picture).collect();
        self.preview = mats
            .iter()
            .enumerate()
            .map(|(i, m)| m.preview_png.as_ref().and_then(|b| image::load_from_memory(b).ok()).map(|im| ctx.load_texture(format!("prev{i}"), to_color_image(&im.to_rgba8(), 256), egui::TextureOptions::LINEAR)))
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
            let small = image::imageops::resize(im, CELL as u32, CELL as u32, image::imageops::FilterType::Triangle);
            image::imageops::replace(&mut atlas, &small, cx as i64, cy as i64);
            let inset = 1.0;
            self.cells[i] = Some([(cx as f32 + inset) / w as f32, (cy as f32 + inset) / hgt as f32, (cx as f32 + CELL as f32 - inset) / w as f32, (cy as f32 + CELL as f32 - inset) / hgt as f32]);
        }
        self.white = [(CELL as f32 * 0.5) / w as f32, (CELL as f32 * 0.5) / hgt as f32];
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
            let spec = d.powf(24.0) * m.maps[3].v * 0.8;
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
        _ => None,
    };
    if let Some(t) = tex {
        painter.image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        return;
    }
    let cells = 14;
    let (cw, ch) = (r.width() / cells as f32, r.height() / cells as f32);
    let paint = |f: &dyn Fn(i32, i32) -> Option<Color32>| {
        for y in 0..cells {
            for x in 0..cells {
                if let Some(c) = f(x, y) {
                    let cr = Rect::from_min_size(r.min + Vec2::new(x as f32 * cw, y as f32 * ch), Vec2::new(cw + 0.5, ch + 0.5));
                    painter.rect_filled(cr, CornerRadius::ZERO, c);
                }
            }
        }
    };
    match slot {
        0 => paint(&|x, y| Some(scale(m.color(), 0.6 + 0.8 * noise(x, y, 11 + m.look as u64)))),
        2 => paint(&|x, y| {
            let n = noise(x, y, 23);
            Some(Color32::from_rgb((100.0 + 80.0 * n) as u8, (90.0 + 60.0 * noise(y, x, 9)) as u8, 240))
        }),
        3 if m.look == 0 => paint(&|x, y| Some(Color32::from_gray((40.0 + 90.0 * noise(x, y, 31) * s.v * 2.0).min(255.0) as u8))),
        8 => paint(&|x, y| Some(Color32::from_gray((235.0 + 20.0 * noise(x, y, 41)) as u8))),
        9 => paint(&|x, y| Some(Color32::from_gray((120.0 + 120.0 * noise(x, y, 51)) as u8))),
        _ => {}
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
        // Sliders fit what's left of the row (their value box takes about 64 px).
        ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(40.0);
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
        1 => material_sets(ui, doc, st),
        2 => meshes(ui, doc, st, pics),
        _ => displacements(ui),
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
    ui.horizontal(|ui| {
        for (i, t) in ["Settings", "Cutouts/Meshes", "UV Areas"].iter().enumerate() {
            ui.selectable_value(&mut st.mat_sub, i, RichText::new(*t).size(12.5));
        }
    });
    let mat_names: Vec<String> = names.clone();
    let mesh_list: Vec<(usize, String)> = doc.meshes.iter().enumerate().filter(|(_, m)| m.material == Some(st.mat)).map(|(i, m)| (i, m.name.clone())).collect();
    let mi = st.mat;
    let Some(m) = doc.materials.get_mut(st.mat) else {
        ui.label(RichText::new("No materials. Add one with ➕.").weak());
        return changed;
    };
    egui::ScrollArea::vertical().id_salt("matscroll").show(ui, |ui| {
        match st.mat_sub {
            1 => {
                ui.label(RichText::new("Cutout meshes drawn on this material").weak());
                for (i, n) in &mesh_list {
                    ui.horizontal(|ui| {
                        ui.label(format!("• {n}"));
                        if ui.small_button("Edit...").clicked() {
                            st.mesh = *i;
                            st.tab = 2;
                        }
                    });
                }
                let mut cut = m.cutout.map_or(0, |c| c + 1);
                ui.horizontal(|ui| {
                    ui.label("Leaves use");
                    let mut opts = vec!["(first mesh)".to_string()];
                    opts.extend(mesh_list.iter().map(|(_, n)| n.clone()));
                    let o: Vec<&str> = opts.iter().map(String::as_str).collect();
                    let mut sel = mesh_list.iter().position(|(i, _)| Some(*i + 1) == Some(cut)).map_or(0, |p| p + 1);
                    if combo(ui, "matcut", &mut sel, &o, 160.0) {
                        cut = if sel == 0 { 0 } else { mesh_list[sel - 1].0 + 1 };
                        m.cutout = cut.checked_sub(1);
                        changed = true;
                    }
                });
                if ui.button("➕ New cutout for this material").clicked() {
                    st.message = Some("new-cutout".into());
                }
            }
            2 => {
                ui.label(RichText::new("UV area: the whole picture, (0, 0) to (1, 1). Named areas aren't supported yet.").weak());
            }
            _ => {
                lrow(ui, 64.0, "Name", |ui| changed |= ui.add(egui::TextEdit::singleline(&mut m.name).desired_width((ui.available_width() - 8.0).max(40.0))).changed());
                let half = (ui.available_width() - 8.0) / 2.0;
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(half);
                        boxed(ui, "Main", |ui| {
                            lrow(ui, 72.0, "Two-sided", |ui| changed |= ui.checkbox(&mut m.two_sided, "").changed());
                            lrow(ui, 72.0, "Season", |ui| changed |= ui.checkbox(&mut m.season, "").changed());
                        });
                        boxed(ui, "Variations", |ui| {
                            ui.horizontal(|ui| {
                                for c in &mut m.variations {
                                    changed |= ui.color_edit_button_srgb(c).changed();
                                }
                            });
                            lrow(ui, 72.0, "Amount", |ui| changed |= ui.add(egui::Slider::new(&mut m.var_amount, 0.0..=1.0)).changed());
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
                            lrow(ui, 72.0, "Spacing", |ui| changed |= ui.add(egui::DragValue::new(&mut m.spacing).speed(0.001)).changed());
                        });
                    });
                    ui.vertical(|ui| {
                        ui.set_width(half);
                        boxed(ui, "Atlas", |ui| {
                            lrow(ui, 72.0, "Scale", |ui| changed |= ui.add(egui::DragValue::new(&mut m.atlas_scale).speed(0.01)).changed());
                            lrow(ui, 72.0, "Handling", |ui| changed |= combo(ui, "handling", &mut m.handling, &["Default", "Shared", "Unique"], ui.available_width() - 6.0));
                            lrow(ui, 72.0, "Name", |ui| changed |= ui.add(egui::TextEdit::singleline(&mut m.atlas_name).desired_width((ui.available_width() - 8.0).max(40.0))).changed());
                        });
                        boxed(ui, "Other", |ui| {
                            lrow(ui, 96.0, "Max resolution", |ui| changed |= combo(ui, "maxres", &mut m.max_res, &["Default", "256", "512", "1024", "2048", "4096"], ui.available_width() - 6.0));
                            lrow(ui, 96.0, "User data", |ui| changed |= ui.add(egui::TextEdit::singleline(&mut m.user_data).desired_width((ui.available_width() - 8.0).max(40.0))).changed());
                            ui.vertical_centered_justified(|ui| {
                                if ui.button("Make New Set...").clicked() {
                                    st.sets.push(format!("{} set", m.name));
                                    st.tab = 1;
                                }
                                if ui.button("Reload Maps").on_hover_text("Read the picture files again").clicked() {
                                    st.message = Some("reload".into());
                                }
                            });
                        });
                    });
                });
                ui.label(RichText::new("Click a picture slot to load an image (Color and Opacity are used in the 3D view).").size(11.5).weak());
                let cw = (ui.available_width() - 4.0 * 4.0) / 5.0;
                for row in MAP_NAMES.chunks(5).enumerate() {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (k, name) in row.1.iter().enumerate() {
                            let i = row.0 * 5 + k;
                            ui.vertical(|ui| {
                                ui.set_width(cw);
                                egui::Frame::new().fill(Color32::from_gray(48)).corner_radius(4).inner_margin(4).show(ui, |ui| {
                                    ui.set_width(cw - 8.0);
                                    ui.label(RichText::new(*name).size(11.5));
                                    let slot = &mut m.maps[i];
                                    if MAP_IS_COLOR[i] {
                                        ui.horizontal(|ui| {
                                            changed |= ui.color_edit_button_srgb(&mut slot.color).changed();
                                        });
                                    } else {
                                        changed |= ui.add_sized([cw - 8.0, 20.0], egui::DragValue::new(&mut slot.v).speed(0.01).range(0.0..=1.0)).changed();
                                    }
                                    ui.horizontal(|ui| {
                                        changed |= ui.checkbox(&mut slot.on, "").changed();
                                        if slot.file.is_some() && ui.small_button("✖").on_hover_text("Remove the picture").clicked() {
                                            slot.file = None;
                                            changed = true;
                                        }
                                    });
                                    let (r, resp) = ui.allocate_exact_size(Vec2::new(cw - 8.0, cw - 12.0), Sense::click());
                                    let mm = m.clone();
                                    swatch(ui.painter(), r, i, &mm, pics, mi);
                                    let tip = m.maps[i].file.clone().unwrap_or_else(|| "Click to load a picture".into());
                                    if resp.on_hover_text(tip).clicked() {
                                        if let Some(f) = pick_image() {
                                            m.maps[i].file = Some(f);
                                            changed = true;
                                        }
                                    }
                                });
                            });
                        }
                    });
                }
            }
        }
    });
    changed
}

fn material_sets(ui: &mut egui::Ui, doc: &Doc, st: &mut EditorState) -> bool {
    let mut sel = 0;
    let (add, _, del) = picker(ui, "setpick", &st.sets, "No material sets", &mut sel);
    if add {
        st.sets.push(format!("Set {}", st.sets.len() + 1));
    }
    if del && !st.sets.is_empty() {
        st.sets.pop();
    }
    ui.add_space(6.0);
    if st.sets.is_empty() {
        ui.label(RichText::new("Material sets group materials so they can be swapped together.").weak());
    }
    for s in &st.sets {
        egui::CollapsingHeader::new(s).default_open(true).show(ui, |ui| {
            for m in &doc.materials {
                ui.label(format!("• {}", m.name));
            }
        });
    }
    false
}

fn displacements(ui: &mut egui::Ui) -> bool {
    let mut sel = 0;
    picker(ui, "disppick", &[], "No displacements", &mut sel);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 150.0), Sense::hover());
    checker(ui.painter(), r, 10.0);
    ui.label(RichText::new("Displacement assets aren't supported yet; branches use the noise displacement in their Displacement tab.").weak());
    false
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
    let Some(m) = doc.meshes.get_mut(st.mesh) else { return changed };
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
    ui.vertical_centered_justified(|ui| {
        if ui.add(egui::Button::new(RichText::new("✏ Edit Mesh...").size(14.0)).min_size(Vec2::new(0.0, 30.0))).clicked() {
            st.edit_mesh = Some(MeshEdit::new(st.mesh, m.clone()));
        }
    });
    let h = (ui.available_height() - 190.0).max(140.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
    let p = ui.painter_at(r);
    checker(&p, r, 10.0);
    let v = View { r, zoom: 1.0, pan: Vec2::ZERO };
    if let Some(t) = m.material.and_then(|i| pics.color.get(i)).and_then(|t| t.as_ref()) {
        p.image(t.id(), v.card(), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    }
    draw_cutout(&p, &v, m, st.mesh_show, Color32::from_rgb(40, 220, 230), false, &[]);
    ui.horizontal(|ui| {
        ui.label("Show");
        combo(ui, "meshshow", &mut st.mesh_show, &["High", "Med", "Low"], 60.0);
        ui.checkbox(&mut st.alignment, "Alignment");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (pp, tt) = cutout::build(m, st.mesh_show);
            ui.label(format!("V:{} F:{}", pp.len(), tt.len()));
        });
    });
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
    Add,
    Select,
    Remove,
    AnchorAdd,
    AnchorRemove,
    Mask,
    Color,
}

/// A working copy of a mesh in the Cutout Editor; Done writes it back, Cancel drops it.
pub struct MeshEdit {
    pub index: usize,
    pub mesh: MeshAsset,
    pub lod: usize,
    pub tool: Tool,
    pub mirror: bool,
    pub selected: Vec<usize>,
    drag: Option<Drag>,
    box_start: Option<Pos2>,
    pub density: usize,
    pub brush_erase: bool,
    pub brush_screen: bool,
    pub brush_size: f32,
    pub falloff: f32,
    pub paint: [u8; 3],
    pub show_all: bool,
    pub show_color: bool,
    pub show_opacity: bool,
    zoom: f32,
    pan: Vec2,
    history: Vec<MeshAsset>,
}

#[derive(Clone, Copy)]
enum Drag {
    Points([f32; 2]),
    Pivot,
    Anchor(usize),
    AnchorDir(usize),
}

impl MeshEdit {
    pub fn new(index: usize, mesh: MeshAsset) -> MeshEdit {
        MeshEdit {
            index,
            mesh,
            lod: 0,
            tool: Tool::Add,
            mirror: false,
            selected: vec![],
            drag: None,
            box_start: None,
            density: 24,
            brush_erase: false,
            brush_screen: false,
            brush_size: 0.08,
            falloff: 0.3,
            paint: [255, 220, 120],
            show_all: false,
            show_color: true,
            show_opacity: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            history: vec![],
        }
    }
    fn save_undo(&mut self) {
        self.history.push(self.mesh.clone());
        if self.history.len() > 60 {
            self.history.remove(0);
        }
    }
    fn pts(&mut self) -> &mut Vec<[f32; 2]> {
        &mut self.mesh.lods[self.lod]
    }
}

fn tool_btn(ui: &mut egui::Ui, ed: &mut MeshEdit, t: Tool, icon: &str, tip: &str) {
    let sel = ed.tool == t;
    if ui.add(egui::Button::new(RichText::new(icon).size(16.0)).selected(sel).min_size(Vec2::new(32.0, 28.0))).on_hover_text(tip).clicked() {
        ed.tool = t;
    }
}

fn act_btn(ui: &mut egui::Ui, icon: &str, tip: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(icon).size(16.0)).min_size(Vec2::new(32.0, 28.0))).on_hover_text(tip).clicked()
}

fn group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.label(RichText::new(title).size(12.0).color(Color32::from_gray(190)));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            add(ui);
        });
    });
    ui.separator();
}

/// The Cutout Editor. Returns the edited mesh when Done is pressed.
pub fn edit_mesh_window(ctx: &egui::Context, st: &mut EditorState, doc: &Doc, pics: &Pics) -> Option<(usize, MeshAsset)> {
    let ed = st.edit_mesh.as_mut()?;
    let mut done = None;
    let mut close = false;
    let screen = ctx.content_rect();
    egui::Window::new(format!("Edit Mesh - {}", ed.mesh.name))
        .id(egui::Id::unique("cutout_editor"))
        .collapsible(false)
        .resizable(true)
        .default_size([screen.width().min(1180.0) * 0.92, screen.height().min(820.0) * 0.86])
        .default_pos(screen.center() - Vec2::new(screen.width().min(1180.0) * 0.46, screen.height().min(820.0) * 0.43))
        .show(ctx, |ui| {
            // Undo inside the dialog.
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
                if let Some(m) = ed.history.pop() {
                    ed.mesh = m;
                    ed.selected.clear();
                }
            }
            // ------------------------------------------------ toolbar
            ui.horizontal_wrapped(|ui| {
                group(ui, "Points", |ui| {
                    tool_btn(ui, ed, Tool::Add, "➕", "Add: click empty space to add a point, drag a point to move it");
                    tool_btn(ui, ed, Tool::Select, "⬚", "Select: drag a box around points, then drag one of them to move them all");
                    tool_btn(ui, ed, Tool::Remove, "➖", "Remove: click or drag over points to delete them");
                    if act_btn(ui, "↺", "Reset: clear all points") {
                        ed.save_undo();
                        ed.pts().clear();
                        ed.selected.clear();
                    }
                    let mirror = ed.mirror;
                    if ui.add(egui::Button::new(RichText::new("⇋").size(16.0)).selected(mirror).min_size(Vec2::new(32.0, 28.0))).on_hover_text("Mirror: make the left side match the right; new points are added on both sides").clicked() {
                        ed.mirror = !ed.mirror;
                        if ed.mirror {
                            ed.save_undo();
                            let m = cutout::mirror(ed.pts());
                            *ed.pts() = m;
                            ed.selected.clear();
                        }
                    }
                    ui.menu_button(RichText::new("⚙ Generate").size(13.0), |ui| {
                        ui.add(egui::Slider::new(&mut ed.density, 4..=120).text("Point density"));
                        for (i, t) in ["Generate slices with center line", "Generate slices", "Generate adaptive points"].iter().enumerate() {
                            if ui.button(*t).clicked() {
                                ed.save_undo();
                                *ed.pts() = cutout::generate(i, ed.density);
                                ed.selected.clear();
                                ui.close();
                            }
                        }
                        ui.label(RichText::new("Without a picture to trace, these make a leaf outline.").size(11.0).weak());
                    });
                });
                group(ui, "Straighten", |ui| {
                    ui.add_enabled(false, egui::Button::new(RichText::new("〰").size(16.0)).min_size(Vec2::new(32.0, 28.0))).on_disabled_hover_text("Straighten isn't built yet (it bends the picture's space to follow a curved leaf).");
                });
                group(ui, "Anchors", |ui| {
                    tool_btn(ui, ed, Tool::AnchorAdd, "⊕", "Add: click to add an anchor; drag its white dot to set the growth direction. Right-click an anchor to change its ID");
                    tool_btn(ui, ed, Tool::AnchorRemove, "⊖", "Remove: click or drag over anchors");
                    if act_btn(ui, "↺", "Reset anchors (none)") {
                        ed.save_undo();
                        ed.mesh.anchors.clear();
                    }
                });
                group(ui, "Paint", |ui| {
                    tool_btn(ui, ed, Tool::Mask, "🖌", "Mask: paint over triangles to remove them (Erase brings them back)");
                    tool_btn(ui, ed, Tool::Color, "🎨", "Color: paint a colour onto the points");
                    ui.color_edit_button_srgb(&mut ed.paint).on_hover_text("Paint colour");
                    ui.menu_button(RichText::new("Clear/Fill").size(13.0), |ui| {
                        if ui.button("Clear mask").clicked() {
                            ed.save_undo();
                            let l = ed.lod;
                            ed.mesh.mask[l].clear();
                            ui.close();
                        }
                        for (t, c) in [("Fill with black", [0u8; 3]), ("Fill with white", [255; 3])] {
                            if ui.button(t).clicked() {
                                ed.save_undo();
                                ed.mesh.colors = vec![c; ed.mesh.lods[0].len()];
                                ui.close();
                            }
                        }
                        if ui.button("Fill with color").clicked() {
                            ed.save_undo();
                            ed.mesh.colors = vec![ed.paint; ed.mesh.lods[0].len()];
                            ui.close();
                        }
                    });
                    ui.add_enabled(false, egui::Button::new(RichText::new("Section").size(13.0))).on_disabled_hover_text("Sections for fronds aren't built yet.");
                });
                if act_btn(ui, "⊡", "Fit the view") {
                    ed.zoom = 1.0;
                    ed.pan = Vec2::ZERO;
                }
            });
            ui.separator();
            let avail = ui.available_size();
            let side_w = 250.0;
            let lod_w = 150.0;
            let canvas = Vec2::new((avail.x - side_w - lod_w - 48.0).max(240.0), (avail.y - 44.0).max(240.0));
            ui.horizontal_top(|ui| {
                // ------------------------------------------------ options
                ui.vertical(|ui| {
                ui.set_width(side_w);
                egui::ScrollArea::vertical().id_salt("cutout_opts").max_height(canvas.y).show(ui, |ui| {
                    ui.set_width(side_w - 12.0);
                    boxed(ui, "Mesh options", |ui| {
                        lrow(ui, 96.0, "Angle", |ui| {
                            ui.add(egui::Slider::new(&mut ed.mesh.angle, -180.0..=180.0).suffix("°"));
                        });
                        lrow(ui, 96.0, "Tessellation", |ui| {
                            combo(ui, "tess", &mut ed.mesh.tess_style, &["Triangular", "Split", "Grid"], 110.0);
                        });
                        lrow(ui, 96.0, "Tess. amount", |ui| {
                            ui.add(egui::Slider::new(&mut ed.mesh.tess_amount, 0.0..=1.0));
                        });
                        lrow(ui, 96.0, "Refine edges", |ui| {
                            ui.add(egui::Slider::new(&mut ed.mesh.refine, 0.0..=1.0));
                        });
                        lrow(ui, 96.0, "Snap threshold", |ui| {
                            ui.add(egui::Slider::new(&mut ed.mesh.snap, 0.0..=1.0));
                        });
                        lrow(ui, 96.0, "Double-sided", |ui| {
                            ui.checkbox(&mut ed.mesh.double_sided, "");
                        });
                    });
                    boxed(ui, "Brush", |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut ed.brush_erase, false, "Mark");
                            ui.selectable_value(&mut ed.brush_erase, true, "Erase");
                        });
                        lrow(ui, 96.0, "Size type", |ui| {
                            let mut i = ed.brush_screen as usize;
                            if combo(ui, "sizetype", &mut i, &["Absolute", "Screen"], 110.0) {
                                ed.brush_screen = i == 1;
                            }
                        });
                        lrow(ui, 96.0, "Size", |ui| {
                            ui.add(egui::Slider::new(&mut ed.brush_size, 0.01..=0.4));
                        });
                        lrow(ui, 96.0, "Falloff", |ui| {
                            ui.add(egui::Slider::new(&mut ed.falloff, 0.0..=1.0));
                        });
                    });
                    boxed(ui, "Material", |ui| {
                        let names: Vec<String> = doc.materials.iter().map(|m| m.name.clone()).collect();
                        let mut opts = vec!["None"];
                        opts.extend(names.iter().map(String::as_str));
                        let mut i = ed.mesh.material.map_or(0, |x| x + 1);
                        if combo(ui, "cutmat", &mut i, &opts, 200.0) {
                            ed.mesh.material = i.checked_sub(1);
                        }
                        lrow(ui, 96.0, "Show all meshes", |ui| {
                            ui.checkbox(&mut ed.show_all, "");
                        });
                        lrow(ui, 96.0, "Show color", |ui| {
                            ui.checkbox(&mut ed.show_color, "");
                        });
                        lrow(ui, 96.0, "Show opacity", |ui| {
                            ui.checkbox(&mut ed.show_opacity, "");
                        });
                    });
                    ui.label(RichText::new("Wheel: zoom · Middle or right drag: pan · Ctrl+Z: undo · drag the white dot to move the pivot").size(11.0).weak());
                });
                });
                // ------------------------------------------------ canvas
                let (r, resp) = ui.allocate_exact_size(canvas, Sense::click_and_drag());
                let p = ui.painter_at(r);
                checker(&p, r, 12.0);
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
                let mat = ed.mesh.material;
                let tex = if ed.show_color { mat.and_then(|i| pics.color.get(i)).and_then(|t| t.as_ref()) } else if ed.show_opacity { mat.and_then(|i| pics.alpha.get(i)).and_then(|t| t.as_ref()) } else { None };
                match tex {
                    Some(t) => {
                        p.image(t.id(), v.card(), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        p.rect_filled(v.card(), CornerRadius::same(4), Color32::from_rgb(214, 250, 252));
                    }
                }
                if ed.show_all {
                    for (i, other) in doc.meshes.iter().enumerate() {
                        if i != ed.index && other.material == ed.mesh.material {
                            let hue = Color32::from_rgb(200, (60 + i * 50 % 180) as u8, 220);
                            draw_cutout(&p, &v, other, 0, hue.gamma_multiply(0.6), false, &[]);
                        }
                    }
                }
                draw_cutout(&p, &v, &ed.mesh, ed.lod, Color32::from_rgb(20, 200, 215), true, &ed.selected);
                // Brush outline.
                let brush_r = |ed: &MeshEdit| if ed.brush_screen { ed.brush_size / ed.zoom } else { ed.brush_size };
                if let Some(h) = resp.hover_pos() {
                    if matches!(ed.tool, Tool::Mask | Tool::Color) {
                        p.circle_stroke(h, brush_r(ed) * v.card().width(), Stroke::new(1.0, Color32::WHITE));
                    }
                }
                if let Some(bs) = ed.box_start {
                    if let Some(h) = resp.hover_pos().or(ui.input(|i| i.pointer.interact_pos())) {
                        p.rect_stroke(Rect::from_two_pos(bs, h), CornerRadius::ZERO, Stroke::new(1.0, Color32::YELLOW), egui::StrokeKind::Inside);
                    }
                }
                // ------------------------------------------------ interaction
                let primary = resp.dragged_by(egui::PointerButton::Primary) || resp.clicked() || resp.drag_started_by(egui::PointerButton::Primary);
                if let (Some(pos), true) = (resp.interact_pointer_pos(), primary) {
                    let q = v.from(pos);
                    let lod = ed.lod;
                    let hit_pt = ed.mesh.lods[lod].iter().position(|w| v.to(*w).distance(pos) < 8.0);
                    if resp.drag_started() || resp.clicked() {
                        ed.drag = None;
                        if v.to(ed.mesh.pivot).distance(pos) < 9.0 {
                            ed.save_undo();
                            ed.drag = Some(Drag::Pivot);
                        } else {
                            match ed.tool {
                                Tool::Add => {
                                    ed.save_undo();
                                    match hit_pt {
                                        Some(i) => {
                                            ed.selected = vec![i];
                                            ed.drag = Some(Drag::Points(q));
                                        }
                                        None => {
                                            ed.pts().push(q);
                                            if ed.mirror && (q[0] - 0.5).abs() > 0.02 {
                                                ed.pts().push([1.0 - q[0], q[1]]);
                                            }
                                            if lod == 0 {
                                                let n = ed.mesh.lods[0].len();
                                                ed.mesh.colors.resize(if ed.mesh.colors.is_empty() { 0 } else { n }, [255; 3]);
                                            }
                                            ed.selected = vec![ed.mesh.lods[lod].len() - 1];
                                            ed.drag = Some(Drag::Points(q));
                                        }
                                    }
                                }
                                Tool::Select => match hit_pt.filter(|i| ed.selected.contains(i)) {
                                    Some(_) => {
                                        ed.save_undo();
                                        ed.drag = Some(Drag::Points(q));
                                    }
                                    None => {
                                        if let Some(i) = hit_pt {
                                            ed.selected = vec![i];
                                            ed.save_undo();
                                            ed.drag = Some(Drag::Points(q));
                                        } else {
                                            ed.box_start = Some(pos);
                                            ed.selected.clear();
                                        }
                                    }
                                },
                                Tool::Remove => ed.save_undo(),
                                Tool::AnchorAdd => {
                                    ed.save_undo();
                                    let hit_dir = ed.mesh.anchors.iter().position(|a| v.to([a.pos[0] + a.dir[0], a.pos[1] + a.dir[1]]).distance(pos) < 8.0);
                                    let hit_a = ed.mesh.anchors.iter().position(|a| v.to(a.pos).distance(pos) < 8.0);
                                    ed.drag = match (hit_dir, hit_a) {
                                        (Some(i), _) => Some(Drag::AnchorDir(i)),
                                        (None, Some(i)) => Some(Drag::Anchor(i)),
                                        _ => {
                                            ed.mesh.anchors.push(Anchor { pos: q, dir: [0.0, 0.15], id: 1 });
                                            Some(Drag::AnchorDir(ed.mesh.anchors.len() - 1))
                                        }
                                    };
                                }
                                Tool::AnchorRemove | Tool::Mask | Tool::Color => ed.save_undo(),
                            }
                        }
                    }
                    // Continuous actions while dragging (and on a click).
                    match ed.drag {
                        Some(Drag::Pivot) => ed.mesh.pivot = q,
                        Some(Drag::Points(prev)) => {
                            let d = [q[0] - prev[0], q[1] - prev[1]];
                            let sel = ed.selected.clone();
                            for i in sel {
                                if let Some(w) = ed.mesh.lods[lod].get_mut(i) {
                                    w[0] += d[0];
                                    w[1] += d[1];
                                }
                            }
                            ed.drag = Some(Drag::Points(q));
                        }
                        Some(Drag::Anchor(i)) => {
                            if let Some(a) = ed.mesh.anchors.get_mut(i) {
                                a.pos = q;
                            }
                        }
                        Some(Drag::AnchorDir(i)) => {
                            if let Some(a) = ed.mesh.anchors.get_mut(i) {
                                a.dir = [q[0] - a.pos[0], q[1] - a.pos[1]];
                            }
                        }
                        None => match ed.tool {
                            Tool::Remove => {
                                if let Some(i) = hit_pt {
                                    ed.mesh.lods[lod].remove(i);
                                    if lod == 0 && i < ed.mesh.colors.len() {
                                        ed.mesh.colors.remove(i);
                                    }
                                    ed.selected.clear();
                                }
                            }
                            Tool::AnchorRemove => {
                                ed.mesh.anchors.retain(|a| v.to(a.pos).distance(pos) >= 8.0);
                            }
                            Tool::Mask => {
                                let br = brush_r(ed);
                                if ed.brush_erase {
                                    ed.mesh.mask[lod].retain(|m| (m[0] - q[0]).hypot(m[1] - q[1]) > br);
                                } else if ed.mesh.mask[lod].last().is_none_or(|m| (m[0] - q[0]).hypot(m[1] - q[1]) > br * 0.3) {
                                    ed.mesh.mask[lod].push([q[0], q[1], br]);
                                }
                            }
                            Tool::Color => {
                                let br = brush_r(ed);
                                let n = ed.mesh.lods[0].len();
                                if ed.mesh.colors.len() != n {
                                    ed.mesh.colors = vec![[255; 3]; n];
                                }
                                for (k, w) in ed.mesh.lods[0].clone().iter().enumerate() {
                                    let d = (w[0] - q[0]).hypot(w[1] - q[1]);
                                    if d < br {
                                        let f = if ed.falloff > 0.0 { ((br - d) / (br * ed.falloff)).clamp(0.0, 1.0) } else { 1.0 };
                                        let target = if ed.brush_erase { [255; 3] } else { ed.paint };
                                        let c = &mut ed.mesh.colors[k];
                                        for j in 0..3 {
                                            c[j] = (c[j] as f32 + (target[j] as f32 - c[j] as f32) * f * 0.5) as u8;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        },
                    }
                }
                if resp.drag_stopped() || resp.clicked() {
                    if let Some(bs) = ed.box_start.take() {
                        if let Some(end) = resp.interact_pointer_pos().or(ui.input(|i| i.pointer.latest_pos())) {
                            let b = Rect::from_two_pos(bs, end);
                            ed.selected = ed.mesh.lods[ed.lod].iter().enumerate().filter(|(_, w)| b.contains(v.to(**w))).map(|(i, _)| i).collect();
                        }
                    }
                    ed.drag = None;
                }
                let del = ui.input(|i| i.key_pressed(egui::Key::Delete));
                if del && !ed.selected.is_empty() && !ui.ctx().egui_wants_keyboard_input() {
                    ed.save_undo();
                    let mut sel = ed.selected.clone();
                    sel.sort_unstable_by(|a, b| b.cmp(a));
                    for i in sel {
                        let lod = ed.lod;
                        if i < ed.mesh.lods[lod].len() {
                            ed.mesh.lods[lod].remove(i);
                        }
                    }
                    ed.selected.clear();
                }
                // Right-click menu on the canvas.
                let anchor_hit = resp.hover_pos().and_then(|h| ed.mesh.anchors.iter().position(|a| v.to(a.pos).distance(h) < 8.0));
                resp.context_menu(|ui| {
                    if let Some(i) = anchor_hit {
                        ui.label("Anchor ID");
                        ui.add(egui::DragValue::new(&mut ed.mesh.anchors[i].id).range(0..=99));
                        ui.separator();
                    }
                    if ui.button("Copy pivot from High").clicked() {
                        ui.close();
                    }
                    if ui.button("Copy points from High").clicked() {
                        ed.save_undo();
                        let l = ed.lod;
                        ed.mesh.lods[l] = ed.mesh.lods[0].clone();
                        ui.close();
                    }
                    if ui.button("Select all points").clicked() {
                        ed.selected = (0..ed.mesh.lods[ed.lod].len()).collect();
                        ui.close();
                    }
                });
                // ------------------------------------------------ LODs
                ui.vertical(|ui| {
                    ui.set_width(lod_w);
                    for (l, name) in ["High", "Med", "Low"].iter().enumerate() {
                        let sel = ed.lod == l;
                        egui::Frame::new().fill(if sel { Color32::from_gray(70) } else { Color32::from_gray(48) }).corner_radius(6).inner_margin(5).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui.selectable_label(sel, RichText::new(*name).strong()).clicked() {
                                    ed.lod = l;
                                    ed.selected.clear();
                                }
                                if l > 0 && ui.small_button("← High").on_hover_text("Copy the High points into this level").clicked() {
                                    ed.save_undo();
                                    ed.mesh.lods[l] = ed.mesh.lods[0].clone();
                                }
                            });
                            let (pr, presp) = ui.allocate_exact_size(Vec2::new(lod_w - 10.0, lod_w - 30.0), Sense::click());
                            let pp = ui.painter_at(pr);
                            checker(&pp, pr, 8.0);
                            let pv = View { r: pr, zoom: 1.0, pan: Vec2::ZERO };
                            if let Some(t) = mat.and_then(|i| pics.color.get(i)).and_then(|t| t.as_ref()) {
                                pp.image(t.id(), pv.card(), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            }
                            draw_cutout(&pp, &pv, &ed.mesh, l, Color32::from_rgb(20, 200, 215), false, &[]);
                            if presp.clicked() {
                                ed.lod = l;
                                ed.selected.clear();
                            }
                            let (vp, vt) = cutout::build(&ed.mesh, l);
                            ui.label(RichText::new(format!("V:{} F:{}", vp.len(), vt.len())).size(11.5));
                        });
                        ui.add_space(4.0);
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                let (vp, vt) = cutout::build(&ed.mesh, ed.lod);
                ui.label(format!("V:{} F:{}   ·   {} selected", vp.len(), vt.len(), ed.selected.len()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("Done").size(14.0)).fill(Color32::from_rgb(0x47, 0x72, 0xb3)).min_size(Vec2::new(110.0, 28.0))).clicked() {
                        done = Some((ed.index, ed.mesh.clone()));
                        close = true;
                    }
                    if ui.add(egui::Button::new(RichText::new("Cancel").size(14.0)).min_size(Vec2::new(110.0, 28.0))).clicked() {
                        close = true;
                    }
                });
            });
        });
    if close {
        st.edit_mesh = None;
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn view_round_trips() {
        let v = View { r: Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0)), zoom: 1.5, pan: Vec2::new(10.0, -5.0) };
        let q = [0.3, 0.7];
        let back = v.from(v.to(q));
        assert!((back[0] - q[0]).abs() < 1e-4 && (back[1] - q[1]).abs() < 1e-4);
    }
}
