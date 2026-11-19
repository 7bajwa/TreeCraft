//! The Tree Window: an orbit camera and a small software renderer that turns the tree mesh
//! into a lit, depth-buffered picture.

use crate::gen::{wind_offset, TreeMesh};
use crate::math::{v3, V3};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};

#[derive(Clone, Copy)]
pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub target: V3,
    pub ortho: bool,
}

impl Camera {
    pub fn fit(min: V3, max: V3) -> Camera {
        let size = (max - min).len().max(0.5);
        Camera {
            yaw: 0.6,
            // Slightly below the crown looking up, so the sky fills most of the view.
            pitch: -0.14,
            dist: size * 1.2,
            target: (min + max) * 0.5 + crate::math::v3(0.0, (max.y - min.y) * 0.04, 0.0),
            ortho: false,
        }
    }
    pub fn basis(&self) -> (V3, V3, V3, V3) {
        let fwd = v3(
            -self.yaw.sin() * self.pitch.cos(),
            -self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        );
        let eye = self.target - fwd * self.dist;
        let right = fwd.cross(V3::UP).norm();
        let up = right.cross(fwd);
        (eye, fwd, right, up)
    }
    pub fn pan(&mut self, dx: f32, dy: f32, h: f32) {
        let (_, _, right, up) = self.basis();
        let k = self.dist / h.max(1.0);
        self.target += right * (-dx * k) + up * (dy * k);
    }
}

/// The material settings the 3D view follows.
#[derive(Clone, Copy, Default)]
pub struct MatLook {
    /// Two-sided off: the back of a leaf gets no direct light.
    pub one_sided: bool,
    /// Season on: the leaves turn with the season.
    pub season: bool,
    /// The Back material's colour, seen on the back of leaves.
    pub back: Option<[u8; 3]>,
    /// Roughness 0 (shiny) .. 1 (matte), Metallic 0..1, AO 0..1 (darkens), Opacity 0..1
    /// (thins leaf pictures), Subsurface colour (light through leaves seen from behind).
    pub rough: f32,
    pub metal: f32,
    pub ao: f32,
    pub opacity: f32,
    pub sss: Option<[u8; 3]>,
}

pub struct Shading {
    pub wireframe: bool,
    pub show_leaves: bool,
    pub grid: bool,
    pub selected: Option<u32>,
    pub time: f32,
    pub wind: f32,
    pub wind_freq: f32,
    pub ref_len: f32,
    pub highlight: Color32,
    pub light: V3,
    pub season: f32,
    pub ao: bool,
    pub ground: bool,
    pub shadow: bool,
    pub show_branches: bool,
    /// Per material: how its leaves look (sides, season, back colour).
    pub mats: Vec<MatLook>,
    /// Instance mode: the picked instance is highlighted instead of the whole generator.
    pub sel_inst: Option<u64>,
    /// Leaf/frond picture atlas (pixels) and the uv cell per material.
    pub atlas: Option<std::sync::Arc<image::RgbaImage>>,
    pub cells: Vec<Option<[f32; 4]>>,
}

// The viewport: blue sky fading to haze, beige ground (estimated from screenshots).
const SKY_TOP: Color32 = Color32::from_rgb(0x6c, 0x9c, 0xe6);
const SKY_HAZE: Color32 = Color32::from_rgb(0xdf, 0xe4, 0xea);
const GROUND_FAR: Color32 = Color32::from_rgb(0xd6, 0xcf, 0xbf);
const GROUND_NEAR: Color32 = Color32::from_rgb(0xb6, 0xa9, 0x8b);
const AUTUMN: Color32 = Color32::from_rgb(205, 112, 38);

pub struct Projector {
    eye: V3,
    fwd: V3,
    right: V3,
    up: V3,
    f: f32,
    c: Pos2,
    ortho: Option<f32>,
}

impl Projector {
    pub fn new(cam: &Camera, rect: Rect) -> Projector {
        let (eye, fwd, right, up) = cam.basis();
        let f = rect.height() * 0.5 / (22.5f32.to_radians()).tan();
        Projector { eye, fwd, right, up, f, c: rect.center(), ortho: cam.ortho.then_some(cam.dist) }
    }
    /// The ray (origin, direction) through a screen point.
    pub fn ray(&self, p: Pos2) -> (V3, V3) {
        let (x, y) = ((p.x - self.c.x) / self.f, (self.c.y - p.y) / self.f);
        match self.ortho {
            Some(d) => (self.eye + self.right * (x * d) + self.up * (y * d), self.fwd),
            None => (self.eye, (self.fwd + self.right * x + self.up * y).norm()),
        }
    }
    /// Screen position and depth; None when behind the camera.
    pub fn project(&self, p: V3) -> Option<(Pos2, f32)> {
        let d = p - self.eye;
        let z = d.dot(self.fwd);
        if z < 0.05 {
            return None;
        }
        let zz = self.ortho.unwrap_or(z);
        let x = d.dot(self.right) / zz * self.f;
        let y = d.dot(self.up) / zz * self.f;
        Some((Pos2::new(self.c.x + x, self.c.y - y), z))
    }
}

/// Which generator and branch instance are under the screen point (nearest triangle).
pub fn pick_inst(cam: &Camera, rect: Rect, mesh: &TreeMesh, pos: Pos2) -> Option<(u32, u64)> {
    let pr = Projector::new(cam, rect);
    let mut best: Option<(f32, u32, u64)> = None;
    for t in &mesh.tris {
        let (Some(a), Some(b), Some(c)) = (pr.project(mesh.v[t[0] as usize].p), pr.project(mesh.v[t[1] as usize].p), pr.project(mesh.v[t[2] as usize].p)) else {
            continue;
        };
        if point_in_tri(pos, a.0, b.0, c.0) {
            let z = (a.1 + b.1 + c.1) / 3.0;
            if best.is_none_or(|(bz, _, _)| z < bz) {
                let v = &mesh.v[t[0] as usize];
                best = Some((z, v.gen, v.inst));
            }
        }
    }
    best.map(|(_, g, i)| (g, i))
}

/// Which generator is under the screen point (nearest triangle), or None for empty space.
pub fn pick(cam: &Camera, rect: Rect, mesh: &TreeMesh, pos: Pos2) -> Option<u32> {
    let pr = Projector::new(cam, rect);
    let mut best: Option<(f32, u32)> = None;
    for t in &mesh.tris {
        let (Some(a), Some(b), Some(c)) = (pr.project(mesh.v[t[0] as usize].p), pr.project(mesh.v[t[1] as usize].p), pr.project(mesh.v[t[2] as usize].p)) else {
            continue;
        };
        if point_in_tri(pos, a.0, b.0, c.0) {
            let z = (a.1 + b.1 + c.1) / 3.0;
            if best.map_or(true, |(bz, _)| z < bz) {
                best = Some((z, mesh.v[t[0] as usize].gen));
            }
        }
    }
    best.map(|(_, g)| g)
}

fn point_in_tri(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let d1 = (p.x - b.x) * (a.y - b.y) - (a.x - b.x) * (p.y - b.y);
    let d2 = (p.x - c.x) * (b.y - c.y) - (b.x - c.x) * (p.y - c.y);
    let d3 = (p.x - a.x) * (c.y - a.y) - (c.x - a.x) * (p.y - a.y);
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn gradient(painter: &egui::Painter, r: Rect, top: Color32, bottom: Color32) {
    let mut m = egui::Mesh::default();
    m.colored_vertex(r.left_top(), top);
    m.colored_vertex(r.right_top(), top);
    m.colored_vertex(r.right_bottom(), bottom);
    m.colored_vertex(r.left_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(m));
}

pub fn draw(painter: &egui::Painter, rect: Rect, cam: &Camera, mesh: &TreeMesh, sh: &Shading) {
    let pr = Projector::new(cam, rect);
    let (eye, fwd, _, up) = cam.basis();
    // Horizon: where the ground plane meets the sky on screen.
    let h = v3(fwd.x, 0.0, fwd.z).norm();
    let horizon = if !sh.ground || cam.ortho || eye.y <= 0.0 || fwd.dot(h) <= 1e-3 {
        rect.bottom()
    } else {
        (pr.c.y - h.dot(up) / h.dot(fwd) * pr.f).clamp(rect.top(), rect.bottom())
    };
    gradient(painter, Rect::from_min_max(rect.min, Pos2::new(rect.right(), horizon)), SKY_TOP, SKY_HAZE);
    if horizon < rect.bottom() {
        gradient(painter, Rect::from_min_max(Pos2::new(rect.left(), horizon), rect.max), GROUND_FAR, GROUND_NEAR);
    }
    if sh.grid && sh.ground {
        draw_grid(painter, &pr, mesh);
    }
    if sh.shadow && sh.ground {
        let c = (mesh.min + mesh.max) * 0.5;
        let r = ((mesh.max.x - mesh.min.x).max(mesh.max.z - mesh.min.z) * 0.45).max(0.2);
        let pts: Option<Vec<Pos2>> = (0..48)
            .map(|i| {
                let a = i as f32 / 48.0 * std::f32::consts::TAU;
                pr.project(v3(c.x + a.cos() * r, 0.0, c.z + a.sin() * r)).map(|p| p.0)
            })
            .collect();
        if let Some(pts) = pts {
            painter.add(egui::Shape::convex_polygon(pts, Color32::from_black_alpha(38), Stroke::NONE));
        }
    }

    // Nothing that changes the picture moved: show the last one (no software render).
    let key = picture_key(cam, rect, painter.ctx().pixels_per_point(), mesh, sh);
    let key_id = egui::Id::new("tree_raster_key");
    if !sh.wireframe && painter.ctx().data(|d| d.get_temp::<u64>(key_id)) == Some(key) {
        if let Some(t) = painter.ctx().data(|d| d.get_temp::<egui::TextureHandle>(egui::Id::new("tree_raster"))) {
            painter.image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            return;
        }
    }
    painter.ctx().data_mut(|d| d.insert_temp(key_id, key));
    let light = sh.light.norm();
    let height = (mesh.max.y - mesh.min.y).max(1e-3);
    let n = mesh.v.len();
    let mut screen: Vec<Option<(Pos2, f32)>> = Vec::with_capacity(n);
    let mut cols: Vec<Color32> = Vec::with_capacity(n);
    let mut albedo: Vec<[f32; 3]> = Vec::with_capacity(n);
    let lin = srgb_table();
    for v in &mesh.v {
        let p = v.p + wind_offset(v, sh.time, sh.wind, sh.wind_freq, sh.ref_len);
        screen.push(pr.project(p));
        let look = if v.two_sided && v.mat > 0 { sh.mats.get(v.mat as usize - 1).copied() } else { None };
        let mut c = Color32::from_rgb(v.c[0], v.c[1], v.c[2]);
        if v.two_sided && sh.season > 0.0 && look.is_none_or(|l| l.season) {
            c = mix(c, AUTUMN, sh.season * 0.8);
        }
        let picked = match sh.sel_inst {
            Some(k) => v.inst == k,
            None => sh.selected == Some(v.gen),
        };
        if picked {
            c = mix(c, sh.highlight, 0.4);
        }
        // Cheap occlusion: lower and inner parts sit in more shade.
        let k = if sh.ao { 0.55 + 0.45 * ((v.p.y - mesh.min.y) / height).clamp(0.0, 1.0) } else { 1.0 };
        albedo.push([lin[c.r() as usize] * k, lin[c.g() as usize] * k, lin[c.b() as usize] * k]);
        cols.push(c);
    }

    let mut order: Vec<(f32, u32)> = Vec::with_capacity(mesh.tris.len());
    for (i, t) in mesh.tris.iter().enumerate() {
        let (Some(a), Some(b), Some(c)) = (screen[t[0] as usize], screen[t[1] as usize], screen[t[2] as usize]) else {
            continue;
        };
        let two = mesh.v[t[0] as usize].two_sided;
        if (two && !sh.show_leaves) || (!two && !sh.show_branches) {
            continue;
        }
        // Two-sided comes from the material: one-sided faces are hidden from behind. Without a
        // material, leaves and fronds show both sides and bark only its outside.
        let tv = &mesh.v[t[0] as usize];
        let both = if tv.mat > 0 { sh.mats.get(tv.mat as usize - 1).map_or(two, |l| !l.one_sided) } else { two };
        if !both && !sh.wireframe {
            let away = if two {
                // Leaves: by their normal (their winding isn't kept in one direction).
                let n = (tv.n + mesh.v[t[1] as usize].n + mesh.v[t[2] as usize].n).norm();
                n.dot(-fwd) < 0.0
            } else {
                (b.0.x - a.0.x) * (c.0.y - a.0.y) - (b.0.y - a.0.y) * (c.0.x - a.0.x) > 0.0
            };
            if away {
                continue;
            }
        }
        let off = |p: Pos2| !rect.expand(50.0).contains(p);
        if off(a.0) && off(b.0) && off(c.0) {
            continue;
        }
        order.push((a.1 + b.1 + c.1, i as u32));
    }
    order.sort_unstable_by(|x, y| y.0.total_cmp(&x.0));

    if sh.wireframe {
        let stroke = egui::Stroke::new(0.6, Color32::from_rgba_unmultiplied(220, 235, 210, 90));
        for &(_, i) in order.iter().rev().take(60_000) {
            let t = mesh.tris[i as usize];
            let p = |k: usize| screen[t[k] as usize].unwrap().0;
            let col = cols[t[0] as usize];
            painter.add(egui::Shape::line(vec![p(0), p(1), p(2), p(0)], egui::Stroke::new(stroke.width, col)));
        }
        return;
    }

    let tris: Vec<u32> = order.iter().map(|o| o.1).collect();
    raster(painter, rect, mesh, sh, &screen, &albedo, &tris, light, -fwd);
}

/// Everything the tree picture depends on, hashed.
fn picture_key(cam: &Camera, rect: Rect, ppp: f32, mesh: &TreeMesh, sh: &Shading) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for x in [cam.yaw, cam.pitch, cam.dist, cam.target.x, cam.target.y, cam.target.z, rect.min.x, rect.min.y, rect.max.x, rect.max.y, ppp, sh.season, sh.light.x, sh.light.y, sh.light.z, sh.ref_len] {
        x.to_bits().hash(&mut h);
    }
    // Only a moving wind needs a new picture every frame.
    if sh.wind > 0.0 {
        (sh.time.to_bits(), sh.wind.to_bits(), sh.wind_freq.to_bits()).hash(&mut h);
    }
    for m in &sh.mats {
        for x in [m.rough, m.metal, m.ao, m.opacity] {
            x.to_bits().hash(&mut h);
        }
        (m.one_sided, m.season, m.back, m.sss).hash(&mut h);
    }
    (mesh.version, cam.ortho, sh.wireframe, sh.show_leaves, sh.show_branches, sh.selected, sh.sel_inst, sh.ao, sh.highlight.to_array()).hash(&mut h);
    sh.atlas.as_ref().map(|a| std::sync::Arc::as_ptr(a) as usize).hash(&mut h);
    sh.cells.len().hash(&mut h);
    h.finish()
}

/// sRGB byte → linear light.
fn srgb_table() -> [f32; 256] {
    let mut t = [0.0; 256];
    for (i, v) in t.iter_mut().enumerate() {
        let c = i as f32 / 255.0;
        *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    }
    t
}

/// Linear light → sRGB byte, after a filmic tone curve (ACES fit) so bright light rolls off.
fn to_srgb(x: f32) -> u8 {
    let x = x.max(0.0) * 0.72;
    let t = (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14);
    let c = t.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0 + 0.5) as u8
}

/// One projected vertex in raster pixels: position, 1/depth, linear albedo, normal, uv.
struct Sv {
    x: f32,
    y: f32,
    iz: f32,
    alb: [f32; 3],
    n: V3,
    uv: [f32; 2],
}

const SUN: [f32; 3] = [2.3, 2.2, 2.0];
const SKY_AMB: [f32; 3] = [0.42, 0.52, 0.72];
const GROUND_AMB: [f32; 3] = [0.24, 0.21, 0.17];

/// Physically based shading of one pixel: GGX highlight with Schlick fresnel, metal/dielectric
/// split, hemisphere sky light, light through leaves, all in linear light.
#[allow(clippy::too_many_arguments)]
fn shade(alb: [f32; 3], n: V3, l: V3, v: V3, rough: f32, metal: f32, ao: f32, leaf: bool, one_sided: bool, sss: [f32; 3]) -> [f32; 3] {
    let ndl = n.dot(l);
    let ndv = n.dot(v).max(1e-3);
    let h = (l + v).norm();
    let ndh = n.dot(h).max(0.0);
    let vdh = v.dot(h).max(0.0);
    let r = rough.clamp(0.04, 1.0);
    let a2 = (r * r) * (r * r);
    let d = a2 / (std::f32::consts::PI * ((ndh * ndh) * (a2 - 1.0) + 1.0).powi(2)).max(1e-6);
    let k = (r + 1.0) * (r + 1.0) / 8.0;
    let dl = ndl.max(0.0);
    let g = (ndv / (ndv * (1.0 - k) + k)) * (dl / (dl * (1.0 - k) + k));
    let fw = (1.0 - vdh).powi(5);
    let up = n.y * 0.5 + 0.5;
    let mut out = [0.0; 3];
    for i in 0..3 {
        let f0 = 0.04 + (alb[i] - 0.04) * metal;
        let f = f0 + (1.0 - f0) * fw;
        let spec = d * g * f / (4.0 * ndv * dl.max(1e-3)) * dl;
        let diff = alb[i] * (1.0 - metal) * dl;
        let amb = alb[i] * (1.0 - metal) * (GROUND_AMB[i] + (SKY_AMB[i] - GROUND_AMB[i]) * up) * ao;
        // Rough or not, a little sky shows in the surface.
        let env = f0 * (1.0 - r) * SKY_AMB[i] * 0.6;
        let through = if leaf { alb[i] * sss[i] * (-ndl).max(0.0) * if one_sided { 0.15 } else { 0.45 } } else { 0.0 };
        out[i] = (diff + spec + through) * SUN[i] + amb + env;
    }
    out
}

/// Depth-buffered software raster of the tree into one picture, so crossing branches and leaves
/// hide each other per pixel, lit per pixel (`shade`). Leaf pictures are alpha-tested at half
/// opacity; bark pictures repeat along the branch. Rows are split across threads.
#[allow(clippy::too_many_arguments)]
fn raster(painter: &egui::Painter, rect: Rect, mesh: &TreeMesh, sh: &Shading, screen: &[Option<(Pos2, f32)>], albedo: &[[f32; 3]], tris: &[u32], light: V3, view: V3) {
    let ctx = painter.ctx();
    let ppp = ctx.pixels_per_point();
    let (w, h) = ((rect.width() * ppp).round().max(1.0) as usize, (rect.height() * ppp).round().max(1.0) as usize);
    let atlas = sh.atlas.as_deref();
    let lin = srgb_table();
    let sv: Vec<Option<Sv>> = mesh
        .v
        .iter()
        .enumerate()
        .map(|(i, v)| screen[i].map(|(p, z)| Sv { x: (p.x - rect.min.x) * ppp, y: (p.y - rect.min.y) * ppp, iz: 1.0 / z, alb: albedo[i], n: v.n, uv: v.uv }))
        .collect();
    let mut px = vec![Color32::TRANSPARENT; w * h];
    let mut depth = vec![0f32; w * h]; // 1/z: bigger is nearer, 0 is empty
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 32);
    let band = h.div_ceil(threads).max(1);
    std::thread::scope(|s| {
        for (k, (pc, dc)) in px.chunks_mut(band * w).zip(depth.chunks_mut(band * w)).enumerate() {
            let sv = &sv;
            let lin = &lin;
            s.spawn(move || {
                let (y0, y1) = (k * band, k * band + pc.len() / w);
                for &ti in tris {
                    let t = mesh.tris[ti as usize];
                    let (Some(a), Some(b), Some(c)) = (&sv[t[0] as usize], &sv[t[1] as usize], &sv[t[2] as usize]) else { continue };
                    let ylo = a.y.min(b.y).min(c.y).floor().max(y0 as f32);
                    let yhi = a.y.max(b.y).max(c.y).ceil().min(y1 as f32 - 1.0);
                    let xlo = a.x.min(b.x).min(c.x).floor().max(0.0);
                    let xhi = a.x.max(b.x).max(c.x).ceil().min(w as f32 - 1.0);
                    let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
                    if yhi < ylo || xhi < xlo || area.abs() < 1e-9 {
                        continue;
                    }
                    // Per triangle: its material's look and picture cell (in atlas pixels).
                    let tv = &mesh.v[t[0] as usize];
                    let leaf = tv.two_sided;
                    let look = if tv.mat > 0 { sh.mats.get(tv.mat as usize - 1).copied() } else { None };
                    let cell = match (atlas, tv.mat) {
                        (Some(at), m) if m > 0 => sh.cells.get(m as usize - 1).copied().flatten().map(|c| [c[0] * at.width() as f32, c[1] * at.height() as f32, c[2] * at.width() as f32, c[3] * at.height() as f32]),
                        _ => None,
                    };
                    let (rough, metal, ao, op) = look.map_or((if leaf { 0.65 } else { 0.85 }, 0.0, 1.0, 1.0), |l| (l.rough, l.metal, l.ao, l.opacity));
                    let one_sided = look.is_some_and(|l| l.one_sided);
                    // A two-sided face seen from behind is lit on that side.
                    let both = look.map_or(leaf, |l| !l.one_sided);
                    let sss = look.and_then(|l| l.sss).map_or([0.9, 1.0, 0.7], |s| s.map(|x| lin[x as usize]));
                    let back_col = look.and_then(|l| l.back).map(|b| b.map(|x| lin[x as usize]));
                    let inv = 1.0 / area;
                    for y in ylo as usize..=yhi as usize {
                        let py = y as f32 + 0.5;
                        let row = (y - y0) * w;
                        for x in xlo as usize..=xhi as usize {
                            let pxf = x as f32 + 0.5;
                            let w0 = ((b.x - pxf) * (c.y - py) - (b.y - py) * (c.x - pxf)) * inv;
                            let w1 = ((c.x - pxf) * (a.y - py) - (c.y - py) * (a.x - pxf)) * inv;
                            let w2 = 1.0 - w0 - w1;
                            // A hair of overlap so shared edges leave no pinholes.
                            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                                continue;
                            }
                            let iz = w0 * a.iz + w1 * b.iz + w2 * c.iz;
                            let i = row + x;
                            if iz <= dc[i] {
                                continue;
                            }
                            let mut alb = [0, 1, 2].map(|k| w0 * a.alb[k] + w1 * b.alb[k] + w2 * c.alb[k]);
                            if let (Some(at), Some(cl)) = (atlas, cell) {
                                let (mut u, mut v) = (w0 * a.uv[0] + w1 * b.uv[0] + w2 * c.uv[0], w0 * a.uv[1] + w1 * b.uv[1] + w2 * c.uv[1]);
                                if leaf {
                                    (u, v) = (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
                                } else {
                                    (u, v) = (u.rem_euclid(1.0), v.rem_euclid(1.0));
                                }
                                // Card space has y up; pictures have y down.
                                let tx = ((cl[0] + (cl[2] - cl[0]) * u) as u32).min(at.width() - 1);
                                let ty = ((cl[1] + (cl[3] - cl[1]) * (1.0 - v)) as u32).min(at.height() - 1);
                                let p = at.get_pixel(tx, ty);
                                if leaf && (p[3] as f32 * op) < 128.0 {
                                    continue;
                                }
                                for k in 0..3 {
                                    alb[k] *= lin[p[k] as usize] * (255.0 / 235.0);
                                }
                            }
                            let mut n = (a.n * w0 + b.n * w1 + c.n * w2).norm();
                            if both && n.dot(view) < 0.0 {
                                n = -n;
                                if let Some(bc) = back_col {
                                    alb = bc;
                                }
                            }
                            let col = shade(alb, n, light, view, rough, metal, ao, leaf, one_sided && leaf, sss);
                            dc[i] = iz;
                            pc[i] = Color32::from_rgb(to_srgb(col[0]), to_srgb(col[1]), to_srgb(col[2]));
                        }
                    }
                }
            });
        }
    });
    let img = egui::ColorImage::new([w, h], px);
    let id = egui::Id::new("tree_raster");
    let tex = match ctx.data_mut(|d| d.get_temp::<egui::TextureHandle>(id)) {
        Some(mut t) => {
            t.set(img, egui::TextureOptions::NEAREST);
            t
        }
        None => {
            let t = ctx.load_texture("tree_raster", img, egui::TextureOptions::NEAREST);
            ctx.data_mut(|d| d.insert_temp(id, t.clone()));
            t
        }
    };
    painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
}

/// Gizmos for forces (arrows, magnets) and collision objects (wire spheres and boxes).
pub fn draw_objects(painter: &egui::Painter, rect: Rect, cam: &Camera, objs: &[crate::model::SceneObj], show_collisions: bool) {
    use crate::model::ObjKind;
    let pr = Projector::new(cam, rect);
    let p3 = |a: [f32; 3]| v3(a[0], a[1], a[2]);
    for o in objs.iter().filter(|o| show_collisions || o.kind.is_force()) {
        let col = if !o.on {
            Color32::from_gray(120)
        } else if o.kind == ObjKind::Knockout {
            Color32::from_rgb(240, 70, 70)
        } else if o.kind.is_force() {
            Color32::from_rgb(90, 200, 255)
        } else {
            Color32::from_rgb(255, 150, 70)
        };
        let s = Stroke::new(1.6, col);
        let c = p3(o.pos);
        let poly = |pts: Vec<V3>| {
            let v: Option<Vec<Pos2>> = pts.iter().map(|&q| pr.project(q).map(|p| p.0)).collect();
            if let Some(v) = v {
                painter.add(egui::Shape::line(v, s));
            }
        };
        let ring = |a: V3, b: V3, r: f32| poly((0..=40).map(|i| { let t = i as f32 / 40.0 * std::f32::consts::TAU; c + a * (t.cos() * r) + b * (t.sin() * r) }).collect());
        let r = o.radius.max(1e-3);
        let (ex, ey, ez) = (v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0), v3(0.0, 0.0, 1.0));
        let axis = p3(o.dir).norm();
        let (u, w) = (axis.perp(), axis.cross(axis.perp()).norm());
        let arrow = |from: V3, to: V3| {
            if let (Some(a), Some(b)) = (pr.project(from), pr.project(to)) {
                painter.arrow(a.0, b.0 - a.0, Stroke::new(2.2, col));
            }
        };
        let cube = |h: [f32; 3]| {
            for (sy, sz) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
                poly(vec![c + v3(-h[0], sy * h[1], sz * h[2]), c + v3(h[0], sy * h[1], sz * h[2])]);
                poly(vec![c + v3(sy * h[0], -h[1], sz * h[2]), c + v3(sy * h[0], h[1], sz * h[2])]);
                poly(vec![c + v3(sy * h[0], sz * h[1], -h[2]), c + v3(sy * h[0], sz * h[1], h[2])]);
            }
        };
        match o.kind {
            ObjKind::Sphere | ObjKind::GeoSphere | ObjKind::Knockout => {
                ring(ex, ey, r);
                ring(ex, ez, r);
                ring(ey, ez, r);
            }
            ObjKind::Box | ObjKind::GeoBox => cube(o.half()),
            ObjKind::Capsule => {
                let tip = c + axis * o.length.max(0.0);
                for k in 0..4 {
                    let a = k as f32 * std::f32::consts::FRAC_PI_2;
                    let off = u * (a.cos() * r) + w * (a.sin() * r);
                    poly(vec![c + off, tip + off]);
                }
                for end in [c, tip] {
                    poly((0..=40).map(|i| { let t = i as f32 / 40.0 * std::f32::consts::TAU; end + u * (t.cos() * r) + w * (t.sin() * r) }).collect());
                }
                // Round ends.
                for (end, s) in [(c, -1.0), (tip, 1.0)] {
                    poly((0..=20).map(|i| { let t = i as f32 / 20.0 * std::f32::consts::PI; end + u * (t.cos() * r) + axis * (s * t.sin() * r) }).collect());
                    poly((0..=20).map(|i| { let t = i as f32 / 20.0 * std::f32::consts::PI; end + w * (t.cos() * r) + axis * (s * t.sin() * r) }).collect());
                }
            }
            ObjKind::Mesh => {
                // Edges between hull corners whose directions are neighbours.
                let pts: Vec<V3> = o.hull.iter().map(|&q| c + p3(q)).collect();
                for i in 0..pts.len() {
                    for j in i + 1..pts.len() {
                        let (a, b) = (o.planes.get(i), o.planes.get(j));
                        if let (Some(a), Some(b)) = (a, b) {
                            if v3(a[0], a[1], a[2]).dot(v3(b[0], b[1], b[2])) > 0.6 {
                                poly(vec![pts[i], pts[j]]);
                            }
                        }
                    }
                }
            }
            ObjKind::Direction | ObjKind::SeasonLight | ObjKind::SeasonWind => {
                arrow(c, c + axis * r);
                if o.kind == ObjKind::SeasonLight {
                    ring(u, w, r * 0.12);
                }
            }
            ObjKind::Magnet => {
                ring(ex, ey, r * 0.15);
                ring(ex, ez, r * 0.15);
            }
            ObjKind::Twist | ObjKind::Curl => {
                ring(u, w, r * 0.5);
                arrow(c - axis * (r * 0.4), c + axis * (r * 0.4));
                if o.kind == ObjKind::Twist {
                    poly((0..=60).map(|i| { let t = i as f32 / 60.0; let a = t * std::f32::consts::TAU * 2.0; c + axis * ((t - 0.5) * r * 0.8) + u * (a.cos() * r * 0.3) + w * (a.sin() * r * 0.3) }).collect());
                }
            }
            ObjKind::Planar => {
                let (a, b) = (u * r, w * r);
                poly(vec![c + a + b, c + a - b, c - a - b, c - a + b, c + a + b]);
                arrow(c, c + axis * (r * 0.4));
            }
            ObjKind::Return => {
                ring(ex, ez, r * 0.25);
                arrow(c, c + ey * (r * 0.5));
            }
            ObjKind::Gnarl => {
                poly((0..=40).map(|i| { let t = i as f32 / 40.0; let a = t * std::f32::consts::TAU * 3.0; c + ex * (a.cos() * r * 0.3 * t) + ez * (a.sin() * r * 0.3 * t) + ey * ((t - 0.5) * r * 0.5) }).collect());
            }
        }
        if let Some((p, _)) = pr.project(c) {
            painter.text(p + egui::vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, &o.name, egui::FontId::proportional(11.0), col);
        }
    }
}

fn draw_grid(painter: &egui::Painter, pr: &Projector, mesh: &TreeMesh) {
    let size = (mesh.max - mesh.min).len().max(1.0);
    let step = 10f32.powf((size / 4.0).log10().floor());
    let n = 10i32;
    let half = step * n as f32;
    let line = |a: V3, b: V3, col: Color32, w: f32| {
        // Split each line so the near end can clip without losing the whole line.
        let segs = 8;
        let mut prev: Option<Pos2> = None;
        for s in 0..=segs {
            let p = a + (b - a) * (s as f32 / segs as f32);
            let cur = pr.project(p).map(|x| x.0);
            if let (Some(p0), Some(p1)) = (prev, cur) {
                painter.line_segment([p0, p1], egui::Stroke::new(w, col));
            }
            prev = cur;
        }
    };
    for i in -n..=n {
        let k = i as f32 * step;
        let (col, w) = if i == 0 {
            (Color32::from_black_alpha(55), 1.2)
        } else {
            (Color32::from_black_alpha(26), 1.0)
        };
        line(v3(k, 0.0, -half), v3(k, 0.0, half), col, w);
        line(v3(-half, 0.0, k), v3(half, 0.0, k), col, w);
    }
}
