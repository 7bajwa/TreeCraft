//! The Tree Window: an orbit camera and a small software renderer that turns the tree mesh
//! into a depth-sorted, lit egui mesh.

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
            // Slightly below the crown looking up, so the sky fills most of the view like the original.
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
    /// Leaf/frond picture atlas: texture, uv cell per material, and a white spot.
    pub atlas: Option<egui::TextureId>,
    pub cells: Vec<Option<[f32; 4]>>,
    pub white: [f32; 2],
}

// The original's viewport: blue sky fading to haze, beige ground (estimated from screenshots).
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

    let light = sh.light.norm();
    let height = (mesh.max.y - mesh.min.y).max(1e-3);
    let n = mesh.v.len();
    let mut screen: Vec<Option<(Pos2, f32)>> = Vec::with_capacity(n);
    let mut cols: Vec<Color32> = Vec::with_capacity(n);
    for v in &mesh.v {
        let p = v.p + wind_offset(v, sh.time, sh.wind, sh.wind_freq, sh.ref_len);
        screen.push(pr.project(p));
        let d = v.n.dot(light);
        let lit = if v.two_sided {
            // Leaves catch light on both sides and let some through.
            0.42 + 0.48 * d.abs() + 0.1 * (v.p.y - mesh.min.y) / (mesh.max.y - mesh.min.y + 1e-3)
        } else {
            0.38 + 0.62 * d.max(0.0) + 0.08 * v.n.y.max(0.0)
        };
        // Pictured leaves take their colour from the picture; lighting still shades them.
        let pictured = v.two_sided && v.mat > 0 && sh.atlas.is_some() && sh.cells.get(v.mat as usize - 1).is_some_and(|c| c.is_some());
        let base = if pictured { [235, 235, 235] } else { v.c };
        let mut c = Color32::from_rgb(
            (base[0] as f32 * lit).min(255.0) as u8,
            (base[1] as f32 * lit).min(255.0) as u8,
            (base[2] as f32 * lit).min(255.0) as u8,
        );
        if sh.ao {
            // Cheap ambient occlusion: lower and inner parts sit in more shade.
            let k = 0.62 + 0.38 * ((v.p.y - mesh.min.y) / height).clamp(0.0, 1.0);
            c = Color32::from_rgb((c.r() as f32 * k) as u8, (c.g() as f32 * k) as u8, (c.b() as f32 * k) as u8);
        }
        if v.two_sided && sh.season > 0.0 {
            c = mix(c, AUTUMN, sh.season * 0.8);
        }
        if sh.selected == Some(v.gen) {
            c = mix(c, sh.highlight, 0.4);
        }
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
        if !two && !sh.wireframe {
            // Back-face cull the bark.
            let area = (b.0.x - a.0.x) * (c.0.y - a.0.y) - (b.0.y - a.0.y) * (c.0.x - a.0.x);
            if area > 0.0 {
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

    // egui meshes are drawn in order, so emitting far-to-near gives correct overlap.
    let mut out = match sh.atlas {
        Some(t) => egui::Mesh::with_texture(t),
        None => egui::Mesh::default(),
    };
    let white = if sh.atlas.is_some() { Pos2::new(sh.white[0], sh.white[1]) } else { egui::epaint::WHITE_UV };
    out.vertices.reserve(order.len() * 3);
    out.indices.reserve(order.len() * 3);
    for &(_, i) in &order {
        let t = mesh.tris[i as usize];
        let base = out.vertices.len() as u32;
        for k in 0..3 {
            let vi = t[k] as usize;
            let v = &mesh.v[vi];
            let cell = if v.two_sided && v.mat > 0 { sh.cells.get(v.mat as usize - 1).copied().flatten() } else { None };
            let uv = match cell {
                // Card space has y up; pictures have y down.
                Some(c) => Pos2::new(c[0] + (c[2] - c[0]) * v.uv[0].clamp(0.0, 1.0), c[1] + (c[3] - c[1]) * (1.0 - v.uv[1].clamp(0.0, 1.0))),
                None => white,
            };
            out.vertices.push(egui::epaint::Vertex { pos: screen[vi].unwrap().0, uv, color: cols[vi] });
        }
        out.add_triangle(base, base + 1, base + 2);
    }
    painter.add(egui::Shape::mesh(out));
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
