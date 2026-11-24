//! Grows the tree: walks the generator graph, places children with the generation modes, shapes
//! each spine (gravity, curl, noise, jink, path, breaks, pruning), then skins branches and builds
//! leaves and fronds from their cutout meshes. Every number is read through the
//! property keys, so variance and parent/profile curves apply the same way everywhere.
//! Formulas and which parts are estimated: MECHANICS.md.

use crate::cutout;
use crate::math::{hash, v3, Rng, V3};
use crate::model::{Doc, DrawnSpline, Gen, InstEdit, Kind, MeshAsset, Mode, ObjKind, SceneObj};
use std::collections::BTreeMap;

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

pub struct Vtx {
    /// The branch instance this vertex belongs to (0 for leaves and fronds).
    pub inst: u64,
    pub p: V3,
    pub n: V3,
    pub c: [u8; 3],
    /// How far wind moves this point (0 at the ground).
    pub flex: f32,
    /// Extra flutter for twigs and leaves, 0 on the main stem.
    pub flutter: f32,
    pub phase: f32,
    pub gen: u32,
    pub two_sided: bool,
    /// Texture position on the material (leaves and fronds).
    pub uv: [f32; 2],
    /// Material index + 1 (0 = none).
    pub mat: u16,
}

#[derive(Default)]
pub struct TreeMesh {
    pub v: Vec<Vtx>,
    pub tris: Vec<[u32; 3]>,
    pub branches: usize,
    pub leaves: usize,
    pub fronds: usize,
    pub min: V3,
    pub max: V3,
    pub truncated: bool,
    pub branch_tris: usize,
    pub leaf_tris: usize,
    pub frond_tris: usize,
    /// Triangles each generator made (for the graph's statistics).
    pub per_gen: HashMap<u32, usize>,
    /// Instances (branches, leaves, fronds) each generator made.
    pub inst_count: HashMap<u32, usize>,
    /// Every branch instance, by key: for picking, gizmos and Hand Draw.
    pub insts: HashMap<u64, InstInfo>,
    /// Different for every build, so the viewport knows when to draw again.
    pub version: u64,
}

/// What Instance mode and Hand Draw need to know about one branch instance.
#[derive(Clone, Default)]
pub struct InstInfo {
    pub gen: u32,
    pub parent: u64,
    pub pts: Vec<V3>,
    pub side: V3,
    pub r0: f32,
}

/// One grown branch: its spine with a frame, radius and wind flex at each point.
struct Inst {
    /// Stable id: the same branch gets the same key every build (Instance mode edits use it).
    key: u64,
    pts: Vec<V3>,
    side: Vec<V3>,
    rad: Vec<f32>,
    flex: Vec<f32>,
    len: f32,
    level: u32,
    phase: f32,
    broken: bool,
    radial: usize,
    lsegs: usize,
}

impl Inst {
    fn root(roll: f32) -> Inst {
        Inst {
            key: 0,
            pts: vec![V3::default(), V3::UP * 1e-3],
            side: vec![v3(roll.cos(), 0.0, roll.sin()); 2],
            rad: vec![0.0, 0.0],
            flex: vec![0.0, 0.0],
            len: 0.0,
            level: 0,
            phase: 0.0,
            broken: false,
            radial: 8,
            lsegs: 1,
        }
    }
    /// Nearest point on the spine to p, and the radius there.
    fn nearest(&self, p: V3) -> (V3, f32) {
        let mut best = (self.pts[0], self.rad[0], f32::MAX);
        for i in 0..self.pts.len().saturating_sub(1) {
            let (a, b) = (self.pts[i], self.pts[i + 1]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.dot(ab).max(1e-12)).clamp(0.0, 1.0);
            let q = a + ab * t;
            let d = (p - q).len();
            if d < best.2 {
                best = (q, self.rad[i] + (self.rad[i + 1] - self.rad[i]) * t, d);
            }
        }
        (best.0, best.1)
    }
    /// Point, tangent, side, radius and flex at t in [0, 1] along the spine.
    fn at(&self, t: f32) -> (V3, V3, V3, f32, f32) {
        let n = self.pts.len() - 1;
        let f = (t.clamp(0.0, 1.0) * n as f32).min(n as f32 - 1e-4);
        let i = (f as usize).min(n - 1);
        let u = f - i as f32;
        let p = self.pts[i] + (self.pts[i + 1] - self.pts[i]) * u;
        let tan = (self.pts[i + 1] - self.pts[i]).norm();
        let side = (self.side[i] + (self.side[i + 1] - self.side[i]) * u).norm();
        let r = self.rad[i] + (self.rad[i + 1] - self.rad[i]) * u;
        let fl = self.flex[i] + (self.flex[i + 1] - self.flex[i]) * u;
        (p, tan, side, r, fl)
    }
}

const MAX_BRANCHES: usize = 40_000;
const MAX_LEAVES: usize = 80_000;
/// Shaping strengths per unit of the property over a whole branch (estimated; MECHANICS.md).
const GRAVITY_K: f32 = 1.6;
const ANCESTOR_K: f32 = 1.2;
/// How hard a strength-1 force bends a branch, like gravity (estimated).
const FORCE_K: f32 = 1.6;

fn p3(a: [f32; 3]) -> V3 {
    v3(a[0], a[1], a[2])
}

/// Bend a growing direction by the scene's forces (`start` is the branch's first direction,
/// `season` 0 summer .. 1 autumn). Strengths and fall-offs are estimated (MECHANICS.md).
pub fn apply_forces(objs: &[SceneObj], pos: V3, d: V3, k: f32, start: V3, season: f32) -> V3 {
    let mut d = d;
    for o in objs.iter().filter(|o| o.on) {
        let c = p3(o.pos);
        let to = c - pos;
        let reach = o.radius.max(1e-3);
        let fall = 1.0 / (1.0 + (to.len() / reach).powi(2));
        let axis = p3(o.dir).norm();
        let push = o.strength * FORCE_K * k;
        match o.kind {
            ObjKind::Direction => d = d + axis * push,
            ObjKind::Magnet => d = d + to.norm() * (push * fall),
            ObjKind::Gnarl => {
                let t = (pos.x + pos.y * 1.7 + pos.z * 2.3) / reach * 4.0 * o.freq.max(0.05);
                d = d + noise3(0x6a71 ^ o.name.len() as u64, t) * (push * 2.0 * fall);
            }
            ObjKind::Twist => d = d.rotate(axis, o.strength * k * PI * fall),
            ObjKind::Curl => d = d + axis.cross(pos - c).norm() * (push * fall),
            ObjKind::Planar => {
                let off = (pos - c).dot(axis);
                let fp = 1.0 / (1.0 + (off.abs() / reach).powi(2));
                d = d - axis * (off.signum() * push * fp);
            }
            ObjKind::Return => d = d + start * push,
            ObjKind::SeasonLight => d = d + axis * (push * (1.0 - season)),
            ObjKind::SeasonWind => d = d + axis * (push * season),
            ObjKind::GeoSphere => {
                let v = pos - c;
                let target = c + v.norm() * reach;
                let gap = (v.len() - reach).abs();
                d = d + (target - pos).norm() * (push / (1.0 + (gap / reach).powi(2)));
            }
            ObjKind::GeoBox => {
                let h = o.half();
                let q = pos - c;
                let inside = q.x.abs() < h[0] && q.y.abs() < h[1] && q.z.abs() < h[2];
                let target = if inside { box_out(q, h) } else { v3(q.x.clamp(-h[0], h[0]), q.y.clamp(-h[1], h[1]), q.z.clamp(-h[2], h[2])) };
                let gap = (target - q).len();
                d = d + (target - q).norm() * (push / (1.0 + (gap / h[0].max(h[1]).max(h[2]).max(1e-3)).powi(2)));
            }
            _ => {}
        }
    }
    d
}

/// An instance's own Move/Rotate/Scale (Instance mode), about its base and in its own axes.
fn inst_transform(pts: &mut [V3], side: &mut V3, dir: V3, e: &InstEdit) {
    let base = pts[0];
    let k = if e.scale > 0.0 { e.scale } else { 1.0 };
    let ax_len = dir.norm();
    let ax_side = side.norm();
    let ax_bin = ax_len.cross(ax_side).norm();
    let turn = |v: V3| v.rotate(ax_len, e.rot[0]).rotate(ax_side, e.rot[1]).rotate(ax_bin, e.rot[2]);
    for q in pts.iter_mut() {
        *q = base + turn((*q - base) * k) + p3(e.offset);
    }
    *side = turn(*side);
}

/// The nearest point on a box's surface from a point inside it (box centred at 0).
fn box_out(q: V3, h: [f32; 3]) -> V3 {
    let (ax, ay, az) = (h[0] - q.x.abs(), h[1] - q.y.abs(), h[2] - q.z.abs());
    let mut out = q;
    if ax <= ay && ax <= az {
        out.x = h[0] * q.x.signum();
    } else if ay <= az {
        out.y = h[1] * q.y.signum();
    } else {
        out.z = h[2] * q.z.signum();
    }
    out
}

/// Whether a Knockout force removes what starts at p.
pub fn knocked_out(objs: &[SceneObj], p: V3) -> bool {
    objs.iter().any(|o| o.on && o.kind == ObjKind::Knockout && (p - p3(o.pos)).len() < o.radius)
}

const EARLY_K: f32 = 0.9;
const LATE_K: f32 = 0.035;

/// Smooth value noise in -1..1 along x for a seed.
fn noise1(seed: u64, x: f32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let h = |k: f32| (hash(seed, k as i64 as u64) >> 40) as f32 / (1u64 << 23) as f32 - 1.0;
    let (a, b) = (h(i), h(i + 1.0));
    let s = f * f * (3.0 - 2.0 * f);
    a + (b - a) * s
}

fn noise3(seed: u64, x: f32) -> V3 {
    v3(noise1(seed, x), noise1(seed ^ 0x5151, x + 17.3), noise1(seed ^ 0xa3a3, x - 31.7))
}

fn noise2(seed: u64, x: f32, y: f32) -> f32 {
    let j = y.floor();
    let f = y - j;
    let s = f * f * (3.0 - 2.0 * f);
    let a = noise1(seed ^ (j as i64 as u64).wrapping_mul(0x9E37), x);
    let b = noise1(seed ^ ((j + 1.0) as i64 as u64).wrapping_mul(0x9E37), x);
    a + (b - a) * s
}

/// Evaluates one node's properties: value ± variance, scaled by the parent curve at the node.
struct Ev<'a> {
    g: &'a Gen,
    trel: f32,
    rng: Rng,
    /// Instance mode: values added for this one instance.
    add: Option<&'a BTreeMap<String, f32>>,
}

impl Ev<'_> {
    /// A random amount in -1..1 spread by the distribution (model::DISTRIBUTIONS).
    fn r(&mut self, dist: u8) -> f32 {
        let r = &mut self.rng;
        match dist {
            // Sum of uniforms: a bell curve in -1..1.
            1 => (r.s() + r.s() + r.s()) / 3.0,
            2 => 2.0 * r.f().sqrt() - 1.0,
            3 => 1.0 - 2.0 * r.f().sqrt(),
            4 => r.s().signum() * r.f().sqrt(),
            5 => r.s().signum(),
            6 => ((r.f() * 5.0).floor().min(4.0) - 2.0) * 0.5,
            _ => r.s(),
        }
    }
    fn val(&mut self, key: &str) -> f32 {
        let p = self.g.pv(key);
        let r = if p.var != 0.0 { self.r(p.dist()) } else { 0.0 };
        (p.v + p.var * r) * self.par(key) + self.add.and_then(|a| a.get(key)).copied().unwrap_or(0.0)
    }
    fn par(&self, key: &str) -> f32 {
        self.g.curve(key, false).map_or(1.0, |c| c.eval(self.trel))
    }
    fn f(&self, key: &str) -> f32 {
        self.g.f(key) + self.add.and_then(|a| a.get(key)).copied().unwrap_or(0.0)
    }
}

fn prof(g: &Gen, key: &str) -> Option<crate::curve::Curve> {
    g.curve(key, true)
}

fn pe(c: &Option<crate::curve::Curve>, u: f32) -> f32 {
    c.as_ref().map_or(1.0, |c| c.eval(u))
}

#[derive(Clone, Copy)]
struct Spot {
    t: f32,
    around: f32,
    trel: f32,
    ext: bool,
    /// For extensions: how far the tip leans off the parent's line (share of a half turn).
    fork: f32,
}

fn spot(t: f32, around: f32, first: f32, last: f32) -> Spot {
    let trel = if (last - first).abs() > 1e-6 { ((t - first) / (last - first)).clamp(0.0, 1.0) } else { 0.5 };
    Spot { t, around, trel, ext: false, fork: 0.0 }
}

/// Spread n children around the parent the way Absolute/Proportional do: 1/n turn
/// apart, Jumble shakes each by up to half that, Sweep scales, then centred and turned by Roll.
fn scatter(g: &Gen, grp: &str, n: usize, first: f32, last: f32, rng: &mut Rng, out: &mut Vec<Spot>) {
    let jumble = g.f(&format!("Generation:{grp}:Jumble"));
    let sweep = g.f(&format!("Generation:{grp}:Sweep"));
    let turn = g.f(&format!("Generation:{grp}:Roll"));
    let arounds: Vec<f32> = (0..n).map(|i| (i as f32 / n as f32 + 0.5 * jumble * rng.s()) * sweep).collect();
    let (lo, hi) = arounds.iter().fold((f32::MAX, f32::MIN), |(l, h), &a| (l.min(a), h.max(a)));
    for a in arounds {
        out.push(spot(first + (last - first) * rng.f(), a - (lo + hi) * 0.5 + turn, first, last));
    }
}

/// Where children sit on a parent.
fn placements(g: &Gen, parent: &Inst, parent_max: f32, rng: &mut Rng) -> Vec<Spot> {
    let mut ev = Ev { g, trel: 0.5, rng: rng.clone(), add: None };
    let first = ev.val("Generation:First").clamp(0.0, 1.0);
    let last = ev.val("Generation:Last").clamp(0.0, 1.0);
    *rng = ev.rng.clone();
    let (first, last) = (first.min(last), last.max(first));
    let roll = rng.f();
    let mut out = Vec::new();
    let len = parent.len.max(1e-3);
    let ratio = if parent_max > 0.0 { (len / parent_max).min(1.0) } else { 1.0 };
    let pv = |k: &str, rng: &mut Rng| {
        let p = g.pv(k);
        p.v + p.var * rng.s()
    };
    match g.mode() {
        Mode::Absolute => {
            let n = pv("Generation:Absolute:Number", rng).trunc().max(0.0) as usize;
            scatter(g, "Absolute", n, first, last, rng, &mut out);
        }
        Mode::Proportional => {
            // the longest parent gets Number children.
            let n = (pv("Generation:Proportional:Number", rng) * ratio + 0.5).trunc().max(0.0) as usize;
            scatter(g, "Proportional", n, first, last, rng, &mut out);
        }
        Mode::Flood => {
            let n = (pv("Generation:Flood:Absolute", rng) + pv("Generation:Flood:+ Relative", rng) * ratio).round().max(0.0) as usize;
            for _ in 0..n {
                out.push(spot(first + (last - first) * rng.f(), rng.f(), first, last));
            }
        }
        Mode::Parent => {
            let n = (pv("Generation:Parent:Absolute", rng) + pv("Generation:Parent:+ Relative", rng) * ratio).round().max(0.0) as usize;
            for i in 0..n {
                out.push(Spot { t: 0.0, around: i as f32 / n.max(1) as f32, trel: 0.0, ext: false, fork: 0.0 });
            }
        }
        Mode::AbsoluteSteps | Mode::ProportionalSteps => {
            // Steps groups between First and Last, trunc(Per step number) in each.
            let grp = if g.mode() == Mode::AbsoluteSteps { "Absolute Steps" } else { "Proportional Steps" };
            let k = |key: &str| format!("Generation:{grp}:{key}");
            let mut steps = pv(&k("Steps"), rng);
            if g.mode() == Mode::ProportionalSteps {
                steps = (steps * ratio + 0.5).trunc(); // no minimum, short parents get none
            }
            let steps = steps.trunc().max(0.0) as usize;
            let spread_key = if g.mode() == Mode::AbsoluteSteps { "Per Step:Spread" } else { "Per Step:Offset" };
            let (spread, step_roll) = (g.f(&k(spread_key)), g.f(&k("Per Step:Roll")));
            let (jumble, sweep, turn) = (g.f(&k("Jumble")), g.f(&k("Sweep")), g.f(&k("Roll")));
            for st in 0..steps {
                // shortcut: steps sit evenly; a distribution curve could place them, add one if needed.
                let u = if steps == 1 { 0.5 } else { st as f32 / (steps - 1) as f32 };
                let n = pv(&k("Per Step:Number"), rng).trunc().max(0.0) as usize;
                let arounds: Vec<f32> = (0..n).map(|i| (0.5 * jumble * rng.s() + i as f32 / n.max(1) as f32) * sweep).collect();
                let (lo, hi) = arounds.iter().fold((f32::MAX, f32::MIN), |(l, h), &a| (l.min(a), h.max(a)));
                for a in arounds {
                    let jit = if g.mode() == Mode::ProportionalSteps {
                        spread * 0.5 * rng.s() // rand·Offset − Offset/2, in parent u
                    } else {
                        spread * 0.5 * rng.s() / steps.max(1) as f32 * 2.0
                    };
                    let tu = (u + jit).clamp(0.0, 1.0);
                    out.push(spot(first + (last - first) * tu, a - (lo + hi) * 0.5 + turn + st as f32 * step_roll + roll, first, last));
                }
            }
        }
        Mode::Phyllotaxy => {
            // groups every Internode length from Last down to First.
            let step = pv("Generation:Phyllotaxy:Nodes:Internode length", rng) / len;
            if step <= 0.0 {
                return out;
            }
            let style = g.idx("Generation:Phyllotaxy:Arrangement:Style");
            let spread = g.f("Generation:Phyllotaxy:Nodes:Spread");
            let spiral_k = g.f("Generation:Phyllotaxy:Nodes:Spiral");
            let limit = g.f("Generation:Phyllotaxy:Nodes:Limit").max(1.0) as u32;
            let mut t = last;
            let (mut node, mut spiral) = (0u32, 0.0);
            while t >= first - 1e-4 && node < limit {
                let (n, base) = match style {
                    0 => (2, 0.0),                       // opposite, all in one plane
                    1 => (2, node as f32 * 0.25),        // opposite, each pair a quarter turn on
                    2 => (2, rng.f()),                   // opposite, random
                    3 => (1, node as f32 * 0.5),         // alternating sides
                    4 => (1, rng.f()),                   // alternating, random
                    _ => (pv("Generation:Phyllotaxy:Arrangement:Children per node", rng).trunc().max(0.0) as usize, 0.0),
                };
                let around = if n > 2 { 1.0 / n as f32 } else { 0.5 };
                for i in 0..n {
                    let tc = (t + spread * step * 0.5 * rng.s()).clamp(0.0, 1.0);
                    out.push(spot(tc, roll + base + i as f32 * around + spiral, first, last));
                }
                t -= step;
                spiral += spiral_k * 0.1;
                node += 1;
            }
        }
        Mode::Interval => {
            // groups every 10/Frequency units from Last down to First.
            let sweep = g.f("Generation:Interval:Sweep");
            let spread = g.f("Generation:Interval:Spread");
            let spiral_k = g.f("Generation:Interval:Spiral");
            let limit = g.f("Generation:Interval:Limit").max(1.0) as u32;
            let balance = g.b("Generation:Interval:Balance");
            let mut t = last;
            let (mut node, mut spiral) = (0u32, 0.0);
            while t >= first - 1e-4 && node < limit {
                let mut ev = Ev { g, trel: spot(t, 0.0, first, last).trel, rng: rng.clone(), add: None };
                let freq = ev.val("Generation:Interval:Frequency");
                let n = ev.val("Generation:Interval:Count").trunc().max(0.0) as usize;
                *rng = ev.rng;
                if freq <= 0.0 {
                    break;
                }
                let step = (10.0 / freq) / len;
                if n > 0 {
                    let around = if n >= 3 { 1.0 / n as f32 } else { 0.5 };
                    let bal = if balance && node % 2 == 1 { 0.5 / n as f32 } else { 0.0 };
                    for i in 0..n {
                        let tc = (t + spread * step * 0.5 * rng.s()).clamp(0.0, 1.0);
                        out.push(spot(tc, roll + i as f32 * around * sweep + spiral + bal, first, last));
                    }
                }
                t -= step;
                spiral += spiral_k * 0.1;
                node += 1;
            }
        }
        Mode::Bifurcation => {
            // the parent lists its bends (sharper than Threshold); each one between
            // First and Last, walked along the parent, gets a child out of the bend's outside.
            // Balance pulls each toward alternating ±0.25 turn sides (wrapped to −0.5..0.4).
            let thr = g.f("Generation:Bifurcation:Threshold");
            let spacing = g.f("Generation:Bifurcation:Spacing");
            let balance = g.f("Generation:Bifurcation:Balance");
            let n = parent.pts.len();
            let w = (n / 12).max(1);
            let mut cand: Vec<(f32, f32, f32)> = vec![];
            for i in w..n.saturating_sub(w) {
                let din = (parent.pts[i] - parent.pts[i - w]).norm();
                let dout = (parent.pts[i + w] - parent.pts[i]).norm();
                let ang = din.dot(dout).clamp(-1.0, 1.0).acos();
                let t = i as f32 / (n - 1) as f32;
                if t < first || t > last || ang < thr * 0.25 {
                    continue;
                }
                let tan = (din + dout).norm();
                let bend = dout - din;
                let outward = (-(bend - tan * bend.dot(tan))).norm();
                let side = parent.side[i];
                let bin = tan.cross(side);
                let a = outward.dot(bin).atan2(outward.dot(side)) / TAU;
                cand.push((ang, t, a));
            }
            let mut taken: Vec<Spot> = vec![];
            let mut side = (rng.f() * 2.0) as u32; // the alternation starts at random
            let mut sharp: Vec<f32> = vec![];
            for (ang, t, mut a) in cand {
                // Within Spacing of the last child, keep only the sharper bend.
                if taken.last().is_some_and(|s| (s.t - t).abs() < spacing.max(1.5 * w as f32 / n as f32)) {
                    if sharp.last().is_some_and(|&s| ang > s) {
                        taken.pop();
                        sharp.pop();
                    } else {
                        continue;
                    }
                }
                sharp.push(ang);
                if balance > 0.0 {
                    let mut alt = roll + if side % 2 == 0 { 0.25 } else { -0.25 };
                    while alt - a > 0.4 {
                        alt -= 1.0;
                    }
                    while alt - a < -0.5 {
                        alt += 1.0;
                    }
                    a += (alt - a) * balance;
                    side += 1;
                }
                taken.push(spot(t, a, first, last));
            }
            out = taken;
        }
        Mode::Classic => {
            // relative style gives Frequency × parent length × 0.025.
            let rel = g.idx("Generation:Style") == 1;
            let mut freq = pv("Generation:Frequency", rng);
            if rel {
                freq *= len * 0.025;
            }
            let steps_on = g.b("Generation:Steps:Enabled");
            let mut steps = if steps_on { pv("Generation:Steps:Number", rng).round().max(1.0) as usize } else { 1 };
            if rel && steps_on {
                steps = ((steps as f32) * ratio).round().max(1.0) as usize;
            }
            let per = if steps_on && g.b("Generation:Steps:Per step frequency") { freq } else { freq / steps as f32 };
            let (balanced, offset, sweep) = (g.b("Generation:Orientation:Balanced"), g.f("Generation:Orientation:Offset"), g.f("Generation:Orientation:Sweep"));
            let (turn, step_spread, step_roll) = (g.f("Generation:Orientation:Roll"), g.f("Generation:Steps:Spread"), g.f("Generation:Steps:Roll"));
            let total = (per * steps as f32).round().max(0.0) as usize;
            let mut made = 0;
            for st in 0..steps {
                let n = if st + 1 == steps { total - made } else { (per.round() as usize).min(total - made) };
                made += n;
                for i in 0..n {
                    let u = if steps_on {
                        let c = (st as f32 + 0.5) / steps as f32;
                        (c + step_spread * rng.s() * 0.5 / steps as f32).clamp(0.0, 1.0)
                    } else {
                        rng.f() // a random draw from the distribution curve
                    };
                    // ((i/n or random) + 0.5·rand(−Offset, Offset)) · Sweep
                    let base = if balanced { i as f32 / n.max(1) as f32 } else { rng.f() };
                    let a = (base + 0.5 * offset * rng.s()) * sweep + turn + st as f32 * step_roll;
                    out.push(spot(first + (last - first) * u, a + roll, first, last));
                }
            }
        }
    }
    out
}

struct Builder<'a> {
    doc: &'a Doc,
    mesh: TreeMesh,
    ref_len: f32,
    trunk_r: f32,
    season: f32,
    /// The point of the year (material sets follow it).
    year: f32,
    lod: f32,
    centre: V3,
    /// Longest instance of each generator, from the previous pass (Proportional mode needs it).
    max_len: HashMap<u32, f32>,
    seen_len: HashMap<u32, f32>,
    cutouts: HashMap<usize, (Vec<[f32; 2]>, Vec<[usize; 3]>)>,
    /// Generation ▸ Knockout: the nodes each generator kept so far.
    crowd: HashMap<u32, Crowd>,
}

/// Points kept so far, in a grid of cells about as wide as the knockout distance, so a crowded
/// node is found without comparing every pair.
#[derive(Default)]
struct Crowd {
    cell: f32,
    grid: HashMap<(i32, i32, i32), Vec<V3>>,
}

impl Crowd {
    /// Whether p lies within `dist` of a point kept before; if not, p is kept.
    fn crowded(&mut self, p: V3, dist: f32) -> bool {
        if self.cell <= 0.0 {
            self.cell = dist.max(1e-4);
        }
        let cell = self.cell;
        let at = |v: f32| (v / cell).floor() as i32;
        // shortcut: looks at most 3 cells out, so nodes far bigger than the first one are under-tested.
        let reach = ((dist / cell).ceil() as i32).clamp(1, 3);
        let (x, y, z) = (at(p.x), at(p.y), at(p.z));
        for dx in -reach..=reach {
            for dy in -reach..=reach {
                for dz in -reach..=reach {
                    if self.grid.get(&(x + dx, y + dy, z + dz)).is_some_and(|v| v.iter().any(|q| (*q - p).len() < dist)) {
                        return true;
                    }
                }
            }
        }
        self.grid.entry((x, y, z)).or_default().push(p);
        false
    }
}

/// Build the tree mesh at an LOD (0 = full detail, 1 = lowest).
pub fn build_lod(doc: &Doc, lod: f32) -> TreeMesh {
    let (mesh, lens) = build_with(doc, HashMap::new(), lod);
    let needs = doc.gens.iter().any(|g| matches!(g.mode(), Mode::Proportional | Mode::ProportionalSteps | Mode::Flood | Mode::Parent | Mode::Classic) && g.kind != Kind::Tree);
    if needs {
        build_with(doc, lens, lod).0
    } else {
        mesh
    }
}

fn build_with(doc: &Doc, max_len: HashMap<u32, f32>, lod: f32) -> (TreeMesh, HashMap<u32, f32>) {
    let tree = doc.tree();
    let lod = if tree.b("Dynamic LOD:Enabled") {
        let n = tree.f("Dynamic LOD:Number").max(1.0);
        (lod * n).round() / n
    } else {
        0.0
    };
    static BUILDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let mut b = Builder {
        doc,
        mesh: TreeMesh { version: BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed), ..TreeMesh::default() },
        ref_len: 0.01,
        trunk_r: 0.001,
        season: doc.season(),
        year: doc.year(),
        lod,
        centre: V3::default(),
        max_len,
        seen_len: HashMap::new(),
        cutouts: HashMap::new(),
        crowd: HashMap::new(),
    };
    let s = doc.size_scalar();
    for g in doc.children(doc.root()) {
        b.ref_len = b.ref_len.max((g.f("Spine:Length:Absolute") + g.pv("Spine:Length:Absolute").var) * s);
        b.trunk_r = b.trunk_r.max(g.f("Skin:Radius:Absolute") * s);
    }
    b.centre = V3::UP * (b.ref_len * 0.6);
    let root = Inst::root(tree.f("Shape:Roll") * PI);
    b.grow(doc.root(), &root, true);
    let sink = tree.f("Shape:Sink");
    let m = &mut b.mesh;
    if sink != 0.0 {
        for v in &mut m.v {
            v.p.y -= sink;
        }
    }
    if m.v.is_empty() {
        m.min = v3(-1.0, 0.0, -1.0);
        m.max = v3(1.0, 2.0, 1.0);
    } else {
        m.min = m.v.iter().fold(m.v[0].p, |a, v| a.min(v.p));
        m.max = m.v.iter().fold(m.v[0].p, |a, v| a.max(v.p));
    }
    (b.mesh, b.seen_len)
}

impl Builder<'_> {
    fn mat_of(&self, g: &Gen) -> (Option<usize>, [u8; 3]) {
        match g.material.filter(|&i| i < self.doc.materials.len()) {
            Some(i) => (Some(i), self.doc.materials[i].color()),
            None => (None, g.color),
        }
    }

    /// The colour a leaf or frond starts from: the material's colour (a light grey when its
    /// picture gives the colour), nudged toward one of the material's three variation colours
    /// by a random share of Amount.
    fn leaf_base(&self, mat: Option<usize>, col: [u8; 3], rng: &mut Rng) -> [u8; 3] {
        let Some(m) = mat.and_then(|i| self.doc.materials.get(i)) else { return col };
        let pictured = m.maps.first().is_some_and(|s| s.has_picture());
        let base = if pictured { [235u8; 3] } else { col };
        if m.var_amount <= 0.0 {
            return base;
        }
        let v = m.variations[(rng.f() * 3.0) as usize % 3];
        // On a picture the variation tints by its hue, at the picture's own brightness.
        let peak = v.iter().copied().max().unwrap_or(1).max(1) as f32;
        let target = if pictured { v.map(|x| (x as f32 * 235.0 / peak) as u8) } else { v };
        let w = m.var_amount.clamp(0.0, 1.0) * rng.f();
        [0, 1, 2].map(|i| (base[i] as f32 + (target[i] as f32 - base[i] as f32) * w) as u8)
    }

    /// Dynamic LOD: whether a node survives at the current LOD (weight favours keeping).
    fn lod_keep(&self, curve_key: &str, weight: f32, rng: &mut Rng) -> bool {
        if self.lod <= 0.0 {
            return true;
        }
        let keep = self.doc.tree().curve(curve_key, true).map_or(1.0, |c| c.eval(self.lod));
        rng.f() < (keep * weight.max(0.0)).clamp(0.0, 1.0)
    }

    fn grow(&mut self, parent_id: u32, parent: &Inst, is_root: bool) {
        let kids: Vec<&Gen> = self.doc.children(parent_id).collect();
        for g in kids {
            let mut rng = Rng::new(hash(g.seed as u64, parent.phase.to_bits() as u64 ^ (parent.pts.len() as u64) << 20));
            let parent_max = self.max_len.get(&parent_id).copied().unwrap_or(0.0);
            let mut spots = placements(g, parent, parent_max, &mut rng);
            self.common(g, parent, is_root, &mut spots, &mut rng);
            // Decoration kinds skin via their render_as() base (estimated geometry for now).
            match g.kind.render_as() {
                Kind::Tree => {}
                Kind::Branch => {
                    for (k, s) in spots.into_iter().enumerate() {
                        if self.mesh.branches >= MAX_BRANCHES {
                            self.mesh.truncated = true;
                            return;
                        }
                        let mut nrng = Rng::new(hash(rng.next_u64(), k as u64));
                        let key = hash(parent.key ^ ((g.id as u64) << 40) ^ 0x5eed, k as u64) | 1;
                        if g.replaced.contains(&key) {
                            continue;
                        }
                        let Some(inst) = self.branch(g, parent, is_root, s, &mut nrng, key) else { continue };
                        self.note_inst(g, parent, &inst);
                        if !s.ext {
                            let seen = self.seen_len.entry(g.id).or_insert(0.0);
                            *seen = seen.max(inst.len);
                        }
                        self.mesh.branches += 1;
                        let visible = !g.hidden && g.idx("Skin:Type") != 3 && Ev { g, trel: s.trel, rng: nrng.clone(), add: None }.val("Skin:Visibility") >= 0.5;
                        if visible {
                            self.skin(g, &inst, (!is_root && !s.ext).then_some(parent), &mut nrng);
                        }
                        self.grow(g.id, &inst, false);
                    }
                    // Hand-drawn splines on this parent.
                    for (di, d) in g.drawn.iter().enumerate().filter(|(_, d)| d.parent == parent.key) {
                        let key = hash(0xd7a3 ^ ((g.id as u64) << 20), di as u64) | 1;
                        let mut nrng = Rng::new(key);
                        let Some(inst) = self.drawn_branch(g, parent, d, key, &mut nrng) else { continue };
                        self.note_inst(g, parent, &inst);
                        self.mesh.branches += 1;
                        if !g.hidden && g.idx("Skin:Type") != 3 {
                            self.skin(g, &inst, (!is_root).then_some(parent), &mut nrng);
                        }
                        self.grow(g.id, &inst, false);
                    }
                }
                Kind::Leaf => {
                    if g.hidden || is_root {
                        continue;
                    }
                    for (k, s) in spots.into_iter().enumerate() {
                        if self.mesh.leaves >= MAX_LEAVES {
                            self.mesh.truncated = true;
                            return;
                        }
                        let mut nrng = Rng::new(hash(rng.next_u64(), k as u64));
                        let before = self.mesh.leaves;
                        self.leaf(g, parent, s, &mut nrng);
                        if self.mesh.leaves > before {
                            *self.mesh.inst_count.entry(g.id).or_default() += 1;
                        }
                    }
                }
                Kind::Frond => {
                    if g.hidden || is_root {
                        continue;
                    }
                    for (k, s) in spots.into_iter().enumerate().take(64) {
                        let mut nrng = Rng::new(hash(rng.next_u64(), k as u64));
                        self.frond(g, parent, s, &mut nrng);
                        *self.mesh.inst_count.entry(g.id).or_default() += 1;
                    }
                }
                // render_as() only yields the four base kinds; this arm is unreachable.
                _ => {}
            }
        }
    }

    /// Shared generation settings: Position, Rotation, Clip, Pruning curve, Extend parent.
    fn common(&self, g: &Gen, parent: &Inst, is_root: bool, spots: &mut Vec<Spot>, rng: &mut Rng) {
        let (first, last) = (g.f("Generation:First"), g.f("Generation:Last"));
        let (lo, hi) = (first.min(last), last.max(first));
        let clip = g.f("Generation:Shared:Boundaries:Clip");
        let pos = g.pv("Generation:Shared:Position");
        let rot = g.pv("Generation:Shared:Rotation");
        let pruning = g.curve("Generation:Pruning", false);
        let shift = pos.v + pos.var * rng.s();
        spots.retain_mut(|s| {
            s.t += shift + pos.var * rng.s() * 0.25;
            s.around += rot.v + rot.var * rng.s();
            if !is_root && (s.t < lo - clip || s.t > hi + clip) {
                return false;
            }
            s.t = s.t.clamp(0.0, 1.0);
            pruning.as_ref().is_none_or(|c| c.eval(s.trel) >= 0.5)
        });
        if !is_root && g.kind != Kind::Frond {
            // Extensions draw from their own random numbers, so turning Extend parent on or off
            // never moves or turns the other children.
            let mut er = Rng::new(hash(g.seed as u64 ^ 0xe7e7, parent.phase.to_bits() as u64));
            let ext = g.idx("Generation:Shared:Extend parent");
            let make = match ext {
                0 => false,
                6 => parent.broken, // Breaks only
                _ => true,
            };
            if make {
                let n = match ext {
                    2 => 1, // Split 1: the tip carries on
                    3 => 2, // Split 2: the tip forks in two
                    _ => {
                        let n = g.pv("Generation:Shared:Extend parent:Count");
                        (n.v + n.var * er.s()).round().max(0.0) as usize
                    }
                };
                let base = er.f();
                for i in 0..n {
                    let t = if ext == 7 { 0.0 } else { 1.0 };
                    let fork = if n > 1 { 0.12 } else { 0.0 };
                    spots.push(Spot { t, around: base + i as f32 / n.max(1) as f32, trel: 1.0, ext: true, fork });
                }
            }
        }
    }

    fn branch(&mut self, g: &Gen, parent: &Inst, is_root: bool, s: Spot, rng: &mut Rng, key: u64) -> Option<Inst> {
        let tree = self.doc.tree();
        let edit = g.inst.get(&key);
        let mut ev = Ev { g, trel: s.trel, rng: rng.clone(), add: edit.map(|e| &e.add) };
        if ev.rng.f() < ev.val("Spine:Prune:General") {
            return None;
        }
        if !self.lod_keep("Dynamic LOD:Branches", ev.val("Dynamic LOD:Weight").max(if parent.level == 0 { 10.0 } else { 0.0 }), &mut ev.rng) {
            return None;
        }
        let size = self.doc.size_scalar() * ev.val("Generation:Size scalar");
        let (mut p, ptan, pside, pr, pflex) = parent.at(s.t);
        if knocked_out(&self.doc.objects, p) {
            return None;
        }
        // Radius first: length may depend on it.
        let mut r0 = ev.val("Skin:Radius:Absolute") * size + ev.val("Skin:Radius:+ % of parent") * pr;
        if s.ext {
            r0 = pr.max(1e-4);
        }
        if g.b("Skin:Radius:Clamp to parent") && pr > 0.0 {
            r0 = r0.min(pr);
        }
        let (rmin, rmax) = (ev.f("Skin:Radius:Clamp:Min"), ev.f("Skin:Radius:Clamp:Max"));
        if rmin > 0.0 {
            r0 = r0.max(rmin);
        }
        if rmax > 0.0 {
            r0 = r0.min(rmax);
        }
        r0 = r0.max(0.0005);
        let mut len = ev.val("Spine:Length:Absolute") * size + ev.val("Spine:Length:+ % of parent") * parent.len + ev.val("Spine:Length:+ % of radius") * r0;
        if s.ext {
            len *= ev.val("Extensions:Spine:Length scale");
        }
        let (lmin, lmax) = (ev.f("Spine:Length:Clamp:Min"), ev.f("Spine:Length:Clamp:Max"));
        if lmin > 0.0 {
            len = len.max(lmin);
        }
        if lmax > 0.0 {
            len = len.min(lmax);
        }
        if g.b("Spine:Length:Match one v tile") {
            let base_r = r0 * pe(&prof(g, "Skin:Radius:Absolute"), 0.0).max(0.0).powf(1.0 / ev.val("Skin:Radius:Length compensation").max(0.05));
            len = TAU * base_r.max(0.0002);
        }
        let kill = ev.val("Skin:Radius:Length kill ratio");
        if len < 1e-3 || (kill > 0.0 && len / r0 > kill) {
            return None;
        }

        // Direction: start angle tilts from the parent's direction toward the outside.
        let radial = pside.rotate(ptan, s.around * TAU).norm();
        let start_angle = if s.ext {
            s.fork
        } else if is_root {
            // Trunks: raw value so 0.5 stands dead upright (parent-curve scaling would tilt it).
            g.f("Spine:Orientation:Start angle").clamp(0.0, 1.0)
        } else {
            ev.val("Spine:Orientation:Start angle").clamp(0.0, 1.0)
        };
        let start_roll = ev.val("Spine:Orientation:Start roll") * PI;
        let mut dir = if is_root {
            // Off the ground: 0.5 stands upright, lower values lean outward.
            let theta = start_angle * PI;
            (radial * theta.cos() + V3::UP * theta.sin()).norm()
        } else {
            let theta = start_angle * PI;
            let bin = ptan.cross(radial).norm();
            let toward = (radial * start_roll.cos() + bin * start_roll.sin()).norm();
            (ptan * theta.cos() + toward * theta.sin()).norm()
        };
        if s.ext && s.t == 0.0 {
            dir = -ptan;
        }
        if is_root {
            // Trunks: First and Last are shares of the tree's Radius out from the middle (Flood
            // spreads evenly over the ground).
            let t = if g.mode() == Mode::Flood { s.t.max(0.0).sqrt() } else { s.t };
            let rr = tree.f("Shape:Radius") * self.doc.size_scalar() * t;
            p += v3(radial.x, 0.0, radial.z).norm() * rr;
        }
        if g.b("Spine:Orientation:Weld start") && !is_root {
            p += radial * pr;
        }
        p += -dir * ev.val("Generation:Sink");

        // Frame: alignment turns the branch's top toward the sky, then Roll spins it.
        let mut side = dir.perp().rotate(dir, ev.rng.f() * TAU);
        let gen_align = match g.mode() {
            Mode::Interval => g.b("Generation:Interval:Align"),
            Mode::Phyllotaxy => g.b("Generation:Phyllotaxy:Arrangement:Align"),
            Mode::Absolute => g.b("Generation:Absolute:Align"),
            Mode::AbsoluteSteps => g.b("Generation:Absolute Steps:Align"),
            Mode::Proportional => g.b("Generation:Proportional:Align"),
            Mode::ProportionalSteps => g.b("Generation:Proportional Steps:Align"),
            _ => true,
        };
        let align = if gen_align { ev.val("Spine:Orientation:Alignment").clamp(0.0, 1.0) } else { 0.0 };
        let up = V3::UP - dir * dir.y;
        if up.len() > 1e-3 {
            side = (side * (1.0 - align) + up.norm() * align).norm();
        }
        side = side.rotate(dir, ev.val("Spine:Orientation:Roll") * PI);
        if g.b("Spine:Orientation:Unify") && s.around.rem_euclid(1.0) >= 0.5 {
            side = -side;
        }

        // Spine shaping at Accuracy steps.
        let n = (g.f("Segments:Accuracy").round() as usize).clamp(4, 400);
        let influence = ev.val("Segments:Influence");
        let step = len / n as f32;
        let grav = ev.val("Spine:Shape:Gravity") + ev.val("Season:Gravity") * self.season;
        let (gc, ac) = (prof(g, "Spine:Shape:Gravity"), prof(g, "Spine:Shape:Ancestor"));
        let anc = ev.val("Spine:Shape:Ancestor");
        let (early, early_turb, early_thr) = (ev.val("Spine:Noise:Early:Amount"), ev.val("Spine:Noise:Early:Turbulence"), ev.val("Spine:Noise:Early:Threshold"));
        let (late, late_turb, late_smooth) = (ev.val("Spine:Noise:Late:Amount"), ev.val("Spine:Noise:Late:Turbulence"), ev.val("Spine:Noise:Late:Smooth"));
        let (ec, lc) = (prof(g, "Spine:Noise:Early:Amount"), prof(g, "Spine:Noise:Late:Amount"));
        let straight = ev.val("Spine:Shape:Straightness").clamp(0.0, 1.0);
        let curl = [ev.val("Spine:Shape:Parent Curl:Up"), ev.val("Spine:Shape:Parent Curl:Right"), ev.val("Spine:Shape:Parent Curl:Out")];
        let curl_c = [prof(g, "Spine:Shape:Parent Curl:Up"), prof(g, "Spine:Shape:Parent Curl:Right"), prof(g, "Spine:Shape:Parent Curl:Out")];
        let zig = ev.val("Spine:Shape:Zigzag:Intensity");
        let zig_n = (ev.val("Spine:Shape:Zigzag:Segment length:Absolute") + ev.val("Spine:Shape:Zigzag:Segment length:+ Relative") * len * 0.1).round().max(1.0);
        let seed = (ev.val("Spine:Noise:Seed").abs() as u64) ^ hash(g.seed as u64, ev.rng.next_u64());
        let right_axis = if is_root { side.cross(dir).norm() } else { ptan.cross(dir).norm() };
        let out_axis = radial;
        let mut pts = Vec::with_capacity(n + 1);
        let mut pos = p;
        let mut d = dir;
        pts.push(pos);
        let mut zig_sign = 1.0;
        let allow_forces = g.b("Forces:Allow forces");
        for i in 0..n {
            let u = i as f32 / n as f32;
            let k = influence / n as f32;
            d = d - V3::UP * (grav * pe(&gc, u) * GRAVITY_K * k);
            if !is_root {
                d = d + ptan * (anc * pe(&ac, u) * ANCESTOR_K * k);
            }
            if early != 0.0 {
                let e = noise3(seed ^ 0xe0, u * (1.0 + early_turb * 12.0));
                if e.len() > early_thr * 0.6 {
                    d = d + e * (early * pe(&ec, u) * EARLY_K * k * (1.0 - straight * (1.0 - u)));
                }
            }
            for (j, axis) in [ptan, right_axis, out_axis].iter().enumerate() {
                if curl[j] != 0.0 && axis.len() > 0.5 {
                    d = d.rotate(*axis, curl[j] * pe(&curl_c[j], u) * PI * k);
                }
            }
            if zig > 0.0 {
                let seg = (u * zig_n).floor() as i32;
                let s2 = if seg % 2 == 0 { 1.0 } else { -1.0 };
                if s2 != zig_sign {
                    d = d.rotate(side, zig * 0.9 * s2 * 2.0);
                    zig_sign = s2;
                } else if i == 0 {
                    d = d.rotate(side, zig * 0.9 * s2);
                }
            }
            if allow_forces {
                d = apply_forces(&self.doc.objects, pos, d, k, dir, self.season);
            }
            d = d.norm();
            pos += d * step;
            pts.push(pos);
        }
        // Late noise: wander added after shaping, held at the base by its profile curve.
        if late != 0.0 {
            let amp = late * LATE_K * len * (1.0 - late_smooth);
            for (i, q) in pts.iter_mut().enumerate() {
                let u = i as f32 / n as f32;
                let off = noise3(seed ^ 0x1a7e, u * (1.0 + late_turb * 8.0)) * (amp * pe(&lc, u) * (1.0 - straight * (1.0 - u)));
                *q += off;
            }
        }
        // Path: a sideways shape traced along the spine, tiled every Length units.
        let path_amt = ev.val("Spine:Path:Amount");
        if path_amt != 0.0 {
            let (pa, pt) = (prof(g, "Spine:Path:Amount"), prof(g, "Spine:Path:Twist"));
            let (plen, poff, ptw) = (ev.val("Spine:Path:Length").max(0.01), ev.val("Spine:Path:Offset"), ev.val("Spine:Path:Twist"));
            for (i, q) in pts.iter_mut().enumerate() {
                let s_along = i as f32 * step;
                let x = (s_along / plen + poff).rem_euclid(1.0);
                let a = (pe(&pa, x) - 0.5) * 2.0 * path_amt;
                let tw = (pe(&pt, x) - 0.5) * 2.0 * ptw * PI;
                *q += side.rotate(dir, tw) * a;
            }
        }
        // Jink: pull toward a polyline of a few long straight pieces.
        let jink = ev.val("Spine:Jink:Amount");
        if jink > 0.0 {
            let pieces = (ev.val("Spine:Jink:Absolute") + ev.val("Spine:Jink:+ Relative") * len).round().max(1.0) as usize;
            let uni = ev.val("Spine:Jink:Uniformity").clamp(0.0, 1.0);
            let jseed = ev.val("Spine:Jink:Seed").abs() as u64;
            let mut jr = Rng::new(jseed ^ seed);
            let mut cuts: Vec<f32> = (1..pieces).map(|k| (k as f32 + (1.0 - uni) * 0.45 * jr.s()) / pieces as f32).collect();
            cuts.insert(0, 0.0);
            cuts.push(1.0);
            let at = |u: f32, pts: &Vec<V3>| pts[((u * n as f32).round() as usize).min(n)];
            let knots: Vec<V3> = cuts.iter().map(|&u| at(u, &pts)).collect();
            let orig = pts.clone();
            for (i, q) in pts.iter_mut().enumerate() {
                let u = i as f32 / n as f32;
                let k = cuts.iter().position(|&c| c >= u).unwrap_or(cuts.len() - 1).max(1);
                let f = ((u - cuts[k - 1]) / (cuts[k] - cuts[k - 1]).max(1e-6)).clamp(0.0, 1.0);
                let line = knots[k - 1] + (knots[k] - knots[k - 1]) * f;
                *q = orig[i] + (line - orig[i]) * jink;
            }
        }
        // Break: snap the branch off at Spot.
        let mut broken = false;
        // The old roll is still drawn so the rest of the branch's random numbers stay put.
        let _ = ev.rng.f();
        let break_roll = Rng::new(hash(ev.f("Spine:Break:Seed").abs() as u64 ^ 0xb4ea, key)).f();
        if break_roll < ev.val("Spine:Break:Chance") {
            let spot_u = ev.val("Spine:Break:Spot").clamp(0.05, 1.0);
            let keep = ((spot_u * n as f32).ceil() as usize).clamp(2, n + 1);
            pts.truncate(keep);
            len *= (keep - 1) as f32 / n as f32;
            broken = true;
        }
        // Bifurcation children need the parent to fork: when the spine has no bend sharp enough
        // for them, kink it at the middle of their First..Last so the top leans one way and the
        // child (placed on the bend's outside) the other, like a Y.
        for c in self.doc.children(g.id).filter(|c| c.mode() == Mode::Bifurcation && !c.hidden) {
            let thr = c.f("Generation:Bifurcation:Threshold") * 0.25;
            let (f, l) = (c.f("Generation:First").clamp(0.0, 1.0), c.f("Generation:Last").clamp(0.0, 1.0));
            let k = (((f + l) * 0.5) * (pts.len() - 1) as f32).round() as usize;
            if k == 0 || k + 1 >= pts.len() {
                continue;
            }
            let w = (pts.len() / 12).max(1);
            let bent = (w..pts.len().saturating_sub(w)).any(|i| (pts[i] - pts[i - w]).norm().dot((pts[i + w] - pts[i]).norm()).clamp(-1.0, 1.0).acos() >= thr.max(0.05));
            if bent {
                continue;
            }
            let tan = (pts[k + 1] - pts[k]).norm();
            let axis = tan.perp().rotate(tan, ev.rng.f() * TAU).cross(tan).norm();
            let ang = (thr * 2.0).max(0.35);
            let pivot = pts[k];
            for q in pts.iter_mut().skip(k + 1) {
                *q = pivot + (*q - pivot).rotate(axis, ang);
            }
        }
        // Prune by direction and by the ground.
        if !is_root {
            let avg = (*pts.last().unwrap() - pts[0]).norm();
            let up_p = ev.val("Spine:Prune:Up");
            let down_p = ev.val("Spine:Prune:Down");
            let int_p = ev.val("Spine:Prune:Interior");
            if ev.rng.f() < up_p * avg.y.max(0.0) || ev.rng.f() < down_p * (-avg.y).max(0.0) {
                return None;
            }
            let outward = v3(pts[0].x, 0.0, pts[0].z);
            if int_p > 0.0 && outward.len() > 1e-3 && ev.rng.f() < int_p * (-v3(avg.x, 0.0, avg.z).norm().dot(outward.norm())).max(0.0) {
                return None;
            }
            if g.b("Spine:Prune:Ground") && pts.iter().any(|q| q.y < -0.01) {
                return None;
            }
        }

        let ko = ev.val("Generation:Knockout");
        if ko > 0.0 && !is_root && self.crowd.entry(g.id).or_default().crowded(pts[pts.len() / 2], ko * len * 0.5) {
            return None;
        }
        // Instance mode: this branch's own move, turn and scale.
        if let Some(e) = edit {
            inst_transform(&mut pts, &mut side, dir, e);
            let k = if e.scale > 0.0 { e.scale } else { 1.0 };
            len *= k;
            r0 *= k;
        }
        // Frames: parallel transport, then Twist turns along the length.
        let twist = ev.val("Spine:Shape:Twist");
        let m = pts.len() - 1;
        let mut sides = Vec::with_capacity(pts.len());
        let mut sd = side;
        for i in 0..=m {
            let tan = if i < m { pts[i + 1] - pts[i] } else { pts[i] - pts[i - 1] }.norm();
            sd = (sd - tan * sd.dot(tan)).norm();
            if !sd.x.is_finite() {
                sd = tan.perp();
            }
            sides.push(sd.rotate(tan, twist * TAU * i as f32 / n as f32));
        }
        let rc = prof(g, "Skin:Radius:Absolute");
        let comp = ev.val("Skin:Radius:Length compensation").max(0.05);
        let wind_w = ev.val("Wind:Legacy:Weight");
        let rad: Vec<f32> = (0..=m).map(|i| (r0 * pe(&rc, i as f32 / n as f32).max(0.0).powf(1.0 / comp)).max(0.0002)).collect();
        let flex: Vec<f32> = (0..=m).map(|i| pflex + wind_w * (i as f32 * step) / self.ref_len).collect();

        // Segment counts (estimated units: MECHANICS.md).
        let scalar = ev.val("Segments:Scalar");
        let lod_l = g.curve("Dynamic LOD:Length segments", true).map_or(1.0, |c| c.eval(self.lod));
        let lod_r = g.curve("Dynamic LOD:Radial segments", true).map_or(1.0, |c| c.eval(self.lod));
        let lbasis = match g.idx("Segments:Length:Relative basis") {
            0 => len,
            1 => len / r0 * 0.25,
            _ => parent.lsegs as f32,
        };
        let lsegs = ((ev.val("Segments:Length:Absolute") + ev.val("Segments:Length: + Relative") * lbasis) * scalar * lod_l).round().clamp(2.0, 120.0) as usize;
        let rbasis = if g.idx("Segments:Radial:Relative basis") == 0 { r0 * 4.0 / self.doc.size_scalar().max(0.01) } else { parent.radial as f32 * 0.5 };
        let radial_n = ((ev.val("Segments:Radial:Absolute") + ev.val("Segments:Radial: + Relative") * rbasis) * scalar * lod_r).round().clamp(3.0, 48.0) as usize;
        *rng = ev.rng;
        Some(Inst { key, pts, side: sides, rad, flex, len, level: parent.level + 1, phase: rng.f() * TAU, broken, radial: radial_n, lsegs })
    }

    fn push_v(&mut self, v: Vtx) -> u32 {
        self.mesh.v.push(v);
        (self.mesh.v.len() - 1) as u32
    }

    fn note_inst(&mut self, g: &Gen, parent: &Inst, inst: &Inst) {
        *self.mesh.inst_count.entry(g.id).or_default() += 1;
        self.mesh.insts.insert(inst.key, InstInfo { gen: g.id, parent: parent.key, pts: inst.pts.clone(), side: inst.side[0], r0: inst.rad[0] });
    }

    /// A hand-drawn spline as a branch: its stroke, evenly spaced, with the generator's skin.
    fn drawn_branch(&mut self, g: &Gen, parent: &Inst, d: &DrawnSpline, key: u64, rng: &mut Rng) -> Option<Inst> {
        let raw: Vec<V3> = d.pts.iter().map(|&q| p3(q)).collect();
        let total: f32 = raw.windows(2).map(|w| (w[1] - w[0]).len()).sum();
        if raw.len() < 2 || total < 1e-4 {
            return None;
        }
        let n = (g.f("Segments:Accuracy").round() as usize).clamp(4, 200);
        let mut pts = Vec::with_capacity(n + 1);
        let (mut seg, mut walked) = (0usize, 0.0f32);
        for i in 0..=n {
            let target = total * i as f32 / n as f32;
            while seg + 2 < raw.len() && walked + (raw[seg + 1] - raw[seg]).len() < target {
                walked += (raw[seg + 1] - raw[seg]).len();
                seg += 1;
            }
            let l = (raw[seg + 1] - raw[seg]).len().max(1e-6);
            let f = ((target - walked) / l).clamp(0.0, 1.0);
            pts.push(raw[seg] + (raw[seg + 1] - raw[seg]) * f);
        }
        let mut ev = Ev { g, trel: 0.5, rng: rng.clone(), add: g.inst.get(&key).map(|e| &e.add) };
        let size = self.doc.size_scalar() * ev.val("Generation:Size scalar");
        let pr = if parent.rad.iter().any(|&r| r > 0.0) { parent.nearest(pts[0]).1 } else { 0.0 };
        let r0 = (ev.val("Skin:Radius:Absolute") * size + ev.val("Skin:Radius:+ % of parent") * pr).max(0.0005);
        let mut sd = (pts[1] - pts[0]).norm().perp();
        let mut sides = Vec::with_capacity(pts.len());
        for i in 0..pts.len() {
            let tan = if i + 1 < pts.len() { pts[i + 1] - pts[i] } else { pts[i] - pts[i - 1] }.norm();
            sd = (sd - tan * sd.dot(tan)).norm();
            if !sd.x.is_finite() {
                sd = tan.perp();
            }
            sides.push(sd);
        }
        let rc = prof(g, "Skin:Radius:Absolute");
        let comp = ev.val("Skin:Radius:Length compensation").max(0.05);
        let rad: Vec<f32> = (0..=n).map(|i| (r0 * pe(&rc, i as f32 / n as f32).max(0.0).powf(1.0 / comp)).max(0.0002)).collect();
        let wind_w = ev.val("Wind:Legacy:Weight");
        let flex: Vec<f32> = (0..=n).map(|i| wind_w * (total * i as f32 / n as f32) / self.ref_len).collect();
        let lsegs = (n / 2).clamp(2, 120);
        let radial = (ev.val("Segments:Radial:Absolute") * ev.val("Segments:Scalar")).round().clamp(3.0, 48.0) as usize;
        *rng = ev.rng;
        Some(Inst { key, pts, side: sides, rad, flex, len: total, level: parent.level + 1, phase: rng.f() * TAU, broken: false, radial, lsegs })
    }

    fn skin(&mut self, g: &Gen, b: &Inst, parent: Option<&Inst>, rng: &mut Rng) {
        let mut ev = Ev { g, trel: 0.5, rng: rng.clone(), add: None };
        // Welding: near the base the skin flares out and every point that would sit inside the
        // parent is laid onto the parent's surface, so the branch grows smoothly out of it.
        let weld = parent.filter(|p| g.b("Skin:Welding:Enabled") && p.rad.iter().any(|&r| r > 0.0));
        let weld_len = b.rad[0].max(1e-4) * 2.0 * ev.val("Skin:Welding:Offset").max(0.05) * (1.0 + 10.0 * ev.val("Skin:Welding:Spread:Amount").max(0.0));
        let weld_flare = ev.val("Skin:Welding:Spread:Upper scale").max(0.0) * 2.0;
        let (mat, col) = self.mat_of(g);
        let col = if mat.and_then(|i| self.doc.materials.get(i)).is_some_and(|m| m.maps.first().is_some_and(|s| s.has_picture())) { [235; 3] } else { col };
        // Rings at even arc length; Optimization drops rings where the spine is straight.
        let mut ring_u: Vec<f32> = (0..=b.lsegs).map(|i| i as f32 / b.lsegs as f32).collect();
        let opt = ev.val("Segments:Length:Optimization");
        if opt > 0.0 {
            let thr = opt * 0.35;
            let mut kept = vec![0.0];
            for w in ring_u.windows(3) {
                let (a, bb, c) = (b.at(w[0]).0, b.at(w[1]).0, b.at(w[2]).0);
                if (bb - a).norm().dot((c - bb).norm()).clamp(-1.0, 1.0).acos() > thr {
                    kept.push(w[1]);
                }
            }
            kept.push(1.0);
            ring_u = kept;
        }
        let radial = b.radial;
        let roll = ev.val("Skin:Roll") * TAU;
        let (sq, sq_rot) = (ev.val("Skin:Squash:Amount").clamp(0.0, 0.95), ev.val("Skin:Squash:Rotate") * PI);
        let lsr = ev.val("Skin:Light seam reduction");
        let dc = prof(g, "Displacement:Amount");
        let sharp = g.curve("Displacement:Sharpness", true);
        let fine_sharp = g.curve("Displacement:Fine:Sharpness", true);
        let disp = ev.val("Displacement:Amount");
        let disp_len = ev.val("Displacement:Length amount");
        let fit = g.idx("Displacement:Mapping") == 1;
        let (us, vs) = (ev.val("Displacement:U scale"), ev.val("Displacement:V scale"));
        let dseed = ev.val("Displacement:Seed").abs() as u64 ^ b.phase.to_bits() as u64;
        let dmap = g.disp_map.and_then(|i| self.doc.displacements.get(i)).and_then(crate::texgen::disp_heights);
        let (fine, fine_scale) = (ev.val("Displacement:Fine:Amount"), ev.val("Displacement:Fine:Scale"));
        let sc = prof(g, "Displacement:Shape:Amount");
        let (shape, shape_tw, shape_n) = (ev.val("Displacement:Shape:Amount"), ev.val("Displacement:Shape:Twist"), ev.val("Displacement:Shape:Scale").round().max(1.0));
        let (fl_r, fl_ns, fl_h, fl_pinch, fl_ang) = (
            ev.val("Displacement:Flares:Radius"),
            ev.val("Displacement:Flares:Noise scale"),
            ev.val("Displacement:Flares:Height").max(0.01),
            ev.val("Displacement:Flares:Pinch").clamp(0.0, 1.0),
            ev.val("Displacement:Flares:Angle") * PI,
        );
        let fseed = ev.val("Displacement:Flares:Seed").abs() as u64;
        let flutter_k = if b.level >= 3 { 1.0 } else { 0.0 };
        let r_base = b.rad[0].max(1e-4);
        let base = self.mesh.v.len() as u32;
        for &u in &ring_u {
            let (p, tan, side, r, fl) = b.at(u);
            let s_along = u * b.len;
            let squash_axis = side.rotate(tan, sq_rot);
            for k in 0..radial {
                let a = k as f32 / radial as f32 * TAU + roll;
                let nrm = side.rotate(tan, a);
                let mut off = nrm * r;
                if sq > 0.0 {
                    off = off - squash_axis * (off.dot(squash_axis) * sq);
                }
                let mut push = 0.0;
                if disp != 0.0 {
                    let (x, y) = if fit { (a / TAU * us * 4.0, u * vs * 4.0) } else { (a / TAU * us * 3.0, s_along / (TAU * r_base) * vs * 3.0) };
                    // A displacement picture wraps once around and repeats along the branch.
                    let nv = match &dmap {
                        Some(m) => m.sample(a / TAU * us.max(0.01), s_along / (TAU * r_base) * vs.max(0.01)),
                        None => noise2(dseed, x, y) * 0.5 + 0.5,
                    };
                    let nv = sharp.as_ref().map_or(nv, |c| c.eval(nv));
                    push += disp * pe(&dc, u) * (nv - 0.5) * 2.0 * r_base * 0.5;
                }
                if fine != 0.0 {
                    let nv = noise2(dseed ^ 0xf1, a / TAU * 12.0 * fine_scale, s_along / r_base * fine_scale) * 0.5 + 0.5;
                    let nv = fine_sharp.as_ref().map_or(nv, |c| c.eval(nv));
                    push += fine * (nv - 0.5) * r_base * 0.4;
                }
                if shape != 0.0 {
                    push += shape * pe(&sc, u) * (a * shape_n + shape_tw * u * TAU).cos() * r * 0.3;
                }
                if fl_r > 0.0 && u < fl_h {
                    let fall = (1.0 - u / fl_h).powf(1.5 + fl_pinch * 2.0);
                    let ridge = (noise1(fseed, (a + fl_ang) / TAU * 6.0 * fl_ns) * 0.5 + 0.5).powf(1.0 + fl_pinch * 3.0);
                    push += fl_r * r_base * fall * ridge;
                }
                let mut pos = p + off + nrm * push + tan * (push * disp_len);
                let mut n_out = nrm;
                if let Some(par) = weld.filter(|_| s_along < weld_len) {
                    let x = 1.0 - s_along / weld_len;
                    let w = x * x * x * (x * (x * 6.0 - 15.0) + 10.0); // smootherstep: 1 at the base, 0 at the end
                    // The flare never grows past the parent's own thickness.
                    let (_, pr_c) = par.nearest(p);
                    let grow = (1.0 + w * weld_flare).min((pr_c * 0.95 / r.max(1e-6)).max(1.0));
                    pos += off * (grow - 1.0);
                    let (q, pr) = par.nearest(pos);
                    let out = pos - q;
                    let d = out.len();
                    if d < pr && d > 1e-6 {
                        pos = q + out * (pr / d);
                    }
                    if d > 1e-6 {
                        n_out = (nrm * (1.0 - w) + out * (w / d)).norm();
                    }
                }
                if lsr > 0.0 {
                    n_out = (n_out + tan * (lsr * (1.0 - u).powi(3) * 2.0)).norm();
                }
                let shade = 0.92 + 0.08 * ((k * 7 + (u * 97.0) as usize * 3) % 5) as f32 / 4.0;
                let c = col.map(|x| (x as f32 * shade) as u8);
                self.push_v(Vtx {
                    p: pos,
                    n: n_out,
                    c,
                    flex: fl,
                    flutter: flutter_k * (fl - b.flex[0]),
                    phase: b.phase,
                    gen: g.id,
                    inst: b.key,
                    two_sided: false,
                    // Bark repeats once per its own circumference along the branch.
                    uv: [k as f32 / radial as f32, s_along / (TAU * r_base)],
                    mat: mat.map_or(0, |m| m as u16 + 1),
                });
            }
        }
        let r = radial as u32;
        let rings = ring_u.len() as u32;
        let mut count = 0;
        for i in 0..rings - 1 {
            for k in 0..r {
                let a = base + i * r + k;
                let bb = base + i * r + (k + 1) % r;
                let c = a + r;
                let d = bb + r;
                self.mesh.tris.push([a, bb, c]);
                self.mesh.tris.push([bb, d, c]);
                count += 2;
            }
        }
        self.mesh.branch_tris += count;
        *self.mesh.per_gen.entry(g.id).or_insert(0) += count;
        *rng = ev.rng;
    }

    fn cutout_of(&mut self, g: &Gen, mesh_key: Option<usize>) -> (usize, Vec<[f32; 2]>, Vec<[usize; 3]>, [f32; 2]) {
        self.cutout_for(g.material, mesh_key)
    }

    /// Which leaf type a leaf uses: the first (the generator's own Material and Mesh, with
    /// Type Weight) or one of the added types, picked at random by weight.
    fn leaf_type(&self, g: &Gen, rng: &mut Rng) -> (Option<usize>, Option<usize>) {
        let pick_set = |set: Option<usize>, mat: Option<usize>, rng: &mut Rng| set.and_then(|j| self.set_pick(j, rng)).or(mat);
        if g.types.is_empty() {
            return (pick_set(g.mat_set, g.material, rng), g.mesh);
        }
        let w0 = g.f("Leaves:Type:0:Weight").max(0.0);
        let total = w0 + g.types.iter().map(|t| t.weight.max(0.0)).sum::<f32>();
        let mut pick = rng.f() * total;
        if pick < w0 || total <= 0.0 {
            return (pick_set(g.mat_set, g.material, rng), g.mesh);
        }
        pick -= w0;
        for t in &g.types {
            if pick < t.weight.max(0.0) {
                let first = pick_set(g.mat_set, g.material, rng);
                return (pick_set(t.set, t.material, rng).or(first), t.mesh.or(g.mesh));
            }
            pick -= t.weight.max(0.0);
        }
        (pick_set(g.mat_set, g.material, rng), g.mesh)
    }

    /// One material of a set: by each member's weight × its share at this point of the year.
    fn set_pick(&self, j: usize, rng: &mut Rng) -> Option<usize> {
        let set = self.doc.material_sets.get(j)?;
        let f = set.factors(self.year);
        let w: Vec<f32> = set.members.iter().zip(&f).map(|(m, f)| if m.material.is_some() { m.weight.max(0.0) * f } else { 0.0 }).collect();
        let total: f32 = w.iter().sum();
        if total <= 0.0 {
            return set.members.iter().find_map(|m| m.material);
        }
        let mut pick = rng.f() * total;
        for (m, w) in set.members.iter().zip(&w) {
            if pick < *w {
                return m.material;
            }
            pick -= w;
        }
        set.members.iter().rev().find_map(|m| m.material)
    }

    fn cutout_for(&mut self, material: Option<usize>, mesh_key: Option<usize>) -> (usize, Vec<[f32; 2]>, Vec<[usize; 3]>, [f32; 2]) {
        let mat_cut = material.and_then(|i| self.doc.materials.get(i)).and_then(|m| m.cutout);
        let idx = mesh_key.or(mat_cut).filter(|&i| i < self.doc.meshes.len()).unwrap_or(0);
        let fallback = MeshAsset::leaf_card();
        let m = self.doc.meshes.get(idx).unwrap_or(&fallback);
        let lod = if self.lod >= 0.99 { 2 } else if self.lod >= 0.5 { 1 } else { 0 };
        let entry = self.cutouts.entry(idx * 3 + lod).or_insert_with(|| {
            let lod = if m.lods[lod].len() >= 3 { lod } else { 0 };
            cutout::build(m, lod)
        });
        (idx, entry.0.clone(), entry.1.clone(), m.cfg(lod).pivot)
    }

    fn leaf(&mut self, g: &Gen, parent: &Inst, s: Spot, rng: &mut Rng) {
        let mut ev = Ev { g, trel: s.trel, rng: rng.clone(), add: None };
        // Season: each leaf changes on its own schedule and falls at Drop time.
        let season = ((self.season - ev.val("Season:Start offset")) * ev.val("Season:Time scale")).clamp(0.0, 1.0);
        let drop = ev.val("Season:Drop time");
        if self.season > 0.0 && drop < 1.0 && season > drop {
            return;
        }
        let tree = self.doc.tree();
        if !self.lod_keep("Dynamic LOD:Leaves", ev.val("Dynamic LOD:Weight"), &mut ev.rng) {
            return;
        }
        let lod_grow = if self.lod > 0.0 {
            let kept = tree.curve("Dynamic LOD:Leaves", true).map_or(1.0, |c| c.eval(self.lod)).max(0.05);
            1.0 + (tree.f("Dynamic LOD:Leaf Settings:Grow scale") - 1.0).max(0.0) * (1.0 - kept) * ev.val("Dynamic LOD:Scale factor")
        } else {
            1.0
        };
        let size = (ev.val("Leaves:Size") + ev.val("Leaves:+ % of parent") * parent.len) * self.doc.size_scalar() * ev.val("Generation:Size scalar") * lod_grow;
        if size <= 1e-4 {
            return;
        }
        let (p, ptan, pside, pr, pflex) = parent.at(s.t);
        let radial = pside.rotate(ptan, s.around * TAU).norm();
        let base = if g.b("Leaves:Weld") { p + radial * pr } else { p + radial * (pr * 0.5) };
        let theta = if s.ext { 0.0 } else { ev.val("Spine:Orientation:Start angle").clamp(0.0, 1.0) * PI };
        // Frame: y along the leaf, x across it, z its face.
        let mut y = (ptan * theta.cos() + radial * theta.sin()).norm();
        let mut x = y.cross(radial).norm();
        if x.len() < 0.5 {
            x = y.perp();
        }
        let mut z = x.cross(y).norm();
        let rot = |x: &mut V3, y: &mut V3, z: &mut V3, axis: V3, a: f32| {
            if a != 0.0 && axis.len() > 0.5 {
                *x = x.rotate(axis, a);
                *y = y.rotate(axis, a);
                *z = z.rotate(axis, a);
            }
        };
        // Local orientation.
        rot(&mut x, &mut y, &mut z, ptan, ev.val("Local Orientation:Fold") * PI * 0.5);
        {
            let ax = x;
            rot(&mut x, &mut y, &mut z, ax, -ev.val("Local Orientation:Align") * PI * 0.5);
        }
        {
            let ax = y;
            rot(&mut x, &mut y, &mut z, ax, ev.val("Local Orientation:Face") * PI);
        }
        let sky = ev.val("Local Orientation:Sky influence");
        if sky != 0.0 {
            let target = if sky > 0.0 { V3::UP } else { -V3::UP };
            let axis = z.cross(target);
            if axis.len() > 1e-3 {
                let ang = z.dot(target).clamp(-1.0, 1.0).acos() * sky.abs().min(1.0);
                let ax = axis.norm();
                rot(&mut x, &mut y, &mut z, ax, ang);
            }
        }
        let sens = ev.val("Sensitivity:Sky");
        if sens > 0.0 {
            // Spin around the leaf's own length so its face points up as far as it can.
            let want = (V3::UP - y * y.y).norm();
            if want.len() > 0.5 {
                let a = z.cross(want).dot(y).atan2(z.dot(want));
                {
            let ax = y;
            rot(&mut x, &mut y, &mut z, ax, a * sens);
        }
            }
        }
        // Season droop and final adjustments.
        let sd = ev.val("Season:Droop") * pe(&prof(g, "Season:Droop"), season) * season;
        {
            let ax = x;
            rot(&mut x, &mut y, &mut z, ax, sd * PI * 0.5);
        }
        {
            let ax = x;
            rot(&mut x, &mut y, &mut z, ax, ev.val("Final Adjustments:Up") * PI);
        }
        {
            let ax = y;
            rot(&mut x, &mut y, &mut z, ax, ev.val("Final Adjustments:Right") * PI);
        }
        {
            let ax = z;
            rot(&mut x, &mut y, &mut z, ax, ev.val("Final Adjustments:Out") * PI);
        }
        if g.b("Leaves:Flip") && ev.rng.f() < 0.5 {
            x = -x;
        }
        let fold = ev.val("Deformation:Fold") + ev.val("Season:Fold") * pe(&prof(g, "Season:Fold"), season) * season;
        let curl = ev.val("Deformation:Curl") + ev.val("Season:Curl") * pe(&prof(g, "Season:Curl"), season) * season;
        let twist = ev.val("Deformation:Twist");
        let twc = prof(g, "Deformation:Twist");
        let (vn_amt, vn_scale) = (ev.val("Deformation:Vertex:Amount"), ev.val("Deformation:Vertex:Noise"));
        let vnc = prof(g, "Deformation:Vertex:Amount");
        let vseed = ev.val("Deformation:Vertex:Seed").abs() as u64 ^ ev.rng.next_u64();
        let (sx, sy, sz) = (ev.val("Deformation:Scale:X"), ev.val("Deformation:Scale:Y"), ev.val("Deformation:Scale:Z"));
        let (puff, world_up, ppuff) = (ev.val("Lighting:Puffiness"), ev.val("Lighting:World up"), ev.val("Lighting:Parent Puffiness:Amount"));
        let wind_w = ev.val("Wind:Weight");
        if knocked_out(&self.doc.objects, base) {
            return;
        }
        let ko = ev.val("Generation:Knockout");
        if ko > 0.0 && self.crowd.entry(g.id).or_default().crowded(base + y * (size * 0.5), ko * size) {
            return;
        }
        let (type_mat, type_mesh) = self.leaf_type(g, &mut ev.rng);
        let (mesh_idx, pts, tris, pivot) = self.cutout_for(type_mat, type_mesh);
        let (mat, col0) = match type_mat.filter(|&i| i < self.doc.materials.len()) {
            Some(i) => (Some(i), self.doc.materials[i].color()),
            None => (None, g.color),
        };
        let mut col = self.leaf_base(mat, col0, &mut ev.rng);
        // The mesh's Transform and Wind direction settings (Meshes tab).
        let (orient, flip, growth, front, scale, ripple_i) = self.doc.meshes.get(mesh_idx).map_or((1, 0, 0, [0.0, 0.0, 1.0], 1.0, 0), |m| (m.orient, m.flip, m.growth, cutout::front(m.orient, m.flip, m.growth, m.flip_normals), m.scale.max(0.01), m.ripple));
        let size = size * scale;
        let ripple = [1.0, 0.0, 2.0][ripple_i.min(2)];
        let tint = 0.85 + 0.3 * ev.rng.f();
        col = col.map(|c| (c as f32 * tint).min(255.0) as u8);
        let paint = self.doc.meshes.get(mesh_idx).map(|m| m.colors.clone()).unwrap_or_default();
        let ph = ev.rng.f() * TAU;
        let copies = if g.b("Leaves:Cross") { 2 } else { 1 };
        let centre_card = pts.iter().fold([0.0, 0.0], |a, q| [a[0] + q[0] / pts.len() as f32, a[1] + q[1] / pts.len() as f32]);
        for copy in 0..copies {
            let (cx, cz) = if copy == 0 { (x, z) } else { (z, -x) };
            let place = |q: [f32; 2]| -> (V3, f32) {
                let a = cutout::axes(orient, flip, growth, [q[0] - pivot[0], q[1] - pivot[1], 0.0]);
                let lx = a[0] * size * sx;
                let ly = a[1] * size * sy;
                let yn = (a[1] / (1.0 - pivot[1]).max(0.1)).clamp(-1.0, 1.0);
                // Fold lifts both halves, curl rolls the tip, twist turns along the length.
                let lz = lx.abs() * (fold * PI * 0.45).tan().clamp(-3.0, 3.0) * sz + a[2] * size;
                let ca = curl * yn.max(0.0) * PI * 0.5;
                let (ly2, lz2) = (ly * ca.cos() - lz * ca.sin(), ly * ca.sin() * sz + lz * ca.cos());
                let ta = twist * pe(&twc, yn.max(0.0)) * PI;
                let (lx3, lz3) = (lx * ta.cos() - lz2 * ta.sin(), lx * ta.sin() + lz2 * ta.cos());
                let mut w = base + cx * lx3 + y * ly2 + cz * lz3;
                if vn_amt != 0.0 {
                    w += noise3(vseed, (q[0] + q[1] * 3.1) * (1.0 + vn_scale * 6.0)) * (vn_amt * pe(&vnc, yn.max(0.0)) * size * 0.15);
                }
                (w, yn.max(0.0))
            };
            let first = self.mesh.v.len() as u32;
            let world: Vec<(V3, f32)> = pts.iter().map(|&q| place(q)).collect();
            // Normals from the triangles touching each point.
            let mut nrm = vec![V3::default(); pts.len()];
            for t in &tris {
                let (a, b, c) = (world[t[0]].0, world[t[1]].0, world[t[2]].0);
                let fnm = (b - a).cross(c - a);
                for &k in t {
                    nrm[k] += fnm;
                }
            }
            let leaf_centre = place(centre_card).0;
            for (k, &q) in pts.iter().enumerate() {
                let (w, yn) = world[k];
                let mut n = if nrm[k].len() > 1e-9 { nrm[k].norm() } else { cz };
                // Normals face the card's front, wherever Orient and Flip turned it.
                if n.dot(cx * front[0] + y * front[1] + cz * front[2]) < 0.0 {
                    n = -n;
                }
                if puff > 0.0 {
                    n = (n * (1.0 - puff) + (w - leaf_centre).norm() * puff).norm();
                }
                if ppuff > 0.0 {
                    n = (n * (1.0 - ppuff) + (w - self.centre).norm() * ppuff).norm();
                }
                if world_up > 0.0 {
                    n = (n * (1.0 - world_up) + V3::UP * world_up).norm();
                }
                let pc = paint.get(k).copied().unwrap_or([255, 255, 255]);
                let c = [0, 1, 2].map(|i| (col[i] as u32 * pc[i] as u32 / 255) as u8);
                let fl = pflex + wind_w * (size * yn) / self.ref_len;
                self.push_v(Vtx { p: w, n, c, flex: fl, flutter: wind_w * ripple * (0.3 + 0.7 * yn), phase: ph, gen: g.id, inst: 0, two_sided: true, uv: q, mat: mat.map_or(0, |m| m as u16 + 1) });
            }
            for t in &tris {
                self.mesh.tris.push([first + t[0] as u32, first + t[1] as u32, first + t[2] as u32]);
            }
            self.mesh.leaf_tris += tris.len();
            *self.mesh.per_gen.entry(g.id).or_insert(0) += tris.len();
        }
        self.mesh.leaves += 1;
        *rng = ev.rng;
    }

    fn frond(&mut self, g: &Gen, parent: &Inst, s: Spot, rng: &mut Rng) {
        let mut ev = Ev { g, trel: s.trel, rng: rng.clone(), add: None };
        let season = ((self.season - ev.val("Season:Start offset")) * ev.val("Season:Time scale")).clamp(0.0, 1.0);
        let drop = ev.val("Season:Drop time");
        if self.season > 0.0 && drop < 1.0 && season > drop {
            return;
        }
        if !self.lod_keep("Dynamic LOD:Fronds", ev.val("Dynamic LOD:Weight"), &mut ev.rng) {
            return;
        }
        let (start, end) = (ev.val("Shape:Boundary:Start").clamp(0.0, 1.0), ev.val("Shape:Boundary:End").clamp(0.0, 1.0));
        let height = ev.val("Shape:Scale:Height").max(0.01);
        let end = start + (end - start) * height;
        let rows = (g.f("Segments:Accuracy").round() as usize).clamp(2, 100);
        let cols = ((ev.val("Segments:Width:Absolute").round() as usize) | 1).clamp(3, 65);
        let width = ev.val("Shape:Scale:Width") * parent.len;
        let wc = prof(g, "Shape:Scale:Width");
        let (grav, fold, curl, roll) = (
            ev.val("Shape:Gravity") + ev.val("Season:Gravity") * season,
            ev.val("Shape:Fold") + ev.val("Season:Fold") * season,
            ev.val("Shape:Curl") + ev.val("Season:Curl") * season,
            ev.val("Shape:Roll"),
        );
        let (gc, cc, rc) = (prof(g, "Shape:Gravity"), prof(g, "Shape:Curl"), prof(g, "Shape:Roll"));
        let lrv = ev.val("Shape:Left-Right Variance:Amount");
        let side_k = [1.0 + lrv * ev.rng.s(), 1.0 + lrv * ev.rng.s()];
        let (l_align, l_spread) = (ev.val("Lighting:Alignment"), ev.val("Lighting:Spread"));
        let wind_w = ev.val("Wind:Weight");
        let flip = g.b("Material:Flip") && ev.rng.f() < 0.5;
        let (_, pts, tris, pivot) = self.cutout_of(g, g.mesh);
        let (ymin, ymax) = pts.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q[1]), b.max(q[1])));
        let (mat, col0) = self.mat_of(g);
        let col = self.leaf_base(mat, col0, &mut ev.rng);
        let ph = ev.rng.f() * TAU;
        let first_v = self.mesh.v.len() as u32;
        let mut grid: Vec<V3> = Vec::with_capacity((rows + 1) * cols);
        let mut uvs = Vec::with_capacity((rows + 1) * cols);
        let mut flexes = Vec::with_capacity((rows + 1) * cols);
        for i in 0..=rows {
            let v = i as f32 / rows as f32;
            let my = ymin + (ymax - ymin) * v.clamp(0.001, 0.999);
            let (lo, hi) = cutout::span_at(&pts, &tris, my).unwrap_or((pivot[0], pivot[0]));
            let t = start + (end - start) * v;
            let (p, tan, side, _, fl) = parent.at(t);
            let normal = side;
            let across = tan.cross(normal).norm();
            let rr = roll * pe(&rc, v) * PI;
            let (across, normal) = (across.rotate(tan, rr), normal.rotate(tan, rr));
            let w = width * pe(&wc, v);
            for j in 0..cols {
                let cu = j as f32 / (cols - 1) as f32;
                let mx = lo + (hi - lo) * cu;
                let mut xo = (mx - pivot[0]) * w;
                if flip {
                    xo = -xo;
                }
                let xn = ((mx - pivot[0]) / 0.5).clamp(-1.0, 1.0);
                let sk = side_k[(xo >= 0.0) as usize];
                let ang = xo.signum() * (fold * PI * 0.5 + curl * pe(&cc, v) * xn.abs() * PI * 0.5) * sk;
                let off = across * (xo.abs() * ang.cos() * xo.signum()) + normal * (xo.abs() * ang.sin());
                let droop = -V3::UP * (grav * pe(&gc, v) * xn * xn * w * sk);
                grid.push(p + off + droop);
                uvs.push([mx, my]);
                flexes.push((fl + wind_w * xo.abs() / self.ref_len, wind_w * (0.4 + 0.6 * v)));
            }
        }
        let idx = |i: usize, j: usize| i * cols + j;
        let mut nrm = vec![V3::default(); grid.len()];
        let mut ftris = vec![];
        for i in 0..rows {
            for j in 0..cols - 1 {
                let (a, b, c, d) = (idx(i, j), idx(i, j + 1), idx(i + 1, j), idx(i + 1, j + 1));
                for t in [[a, c, b], [b, c, d]] {
                    let fnm = (grid[t[1]] - grid[t[0]]).cross(grid[t[2]] - grid[t[0]]);
                    for &k in &t {
                        nrm[k] += fnm;
                    }
                    ftris.push(t);
                }
            }
        }
        for k in 0..grid.len() {
            let i = k / cols;
            let (_, tan, side, _, _) = parent.at(start + (end - start) * i as f32 / rows as f32);
            let mut n = if nrm[k].len() > 1e-9 { nrm[k].norm() } else { side };
            if n.dot(side) < 0.0 {
                n = -n;
            }
            if l_align > 0.0 {
                n = (n * (1.0 - l_align) + tan * l_align).norm();
            }
            if l_spread > 0.0 {
                let out = grid[k] - parent.at(start + (end - start) * i as f32 / rows as f32).0;
                if out.len() > 1e-6 {
                    n = (n * (1.0 - l_spread) + out.norm() * l_spread).norm();
                }
            }
            let tint = 0.9 + 0.12 * ((i as f32 / rows as f32) - 0.5);
            let c = col.map(|x| (x as f32 * tint).min(255.0) as u8);
            self.push_v(Vtx { p: grid[k], n, c, flex: flexes[k].0, flutter: flexes[k].1, phase: ph, gen: g.id, inst: 0, two_sided: true, uv: uvs[k], mat: mat.map_or(0, |m| m as u16 + 1) });
        }
        let two = g.b("Material:Two sided");
        for t in &ftris {
            self.mesh.tris.push([first_v + t[0] as u32, first_v + t[1] as u32, first_v + t[2] as u32]);
            if two {
                self.mesh.tris.push([first_v + t[0] as u32, first_v + t[2] as u32, first_v + t[1] as u32]);
            }
        }
        let made = ftris.len() * if two { 2 } else { 1 };
        self.mesh.frond_tris += made;
        *self.mesh.per_gen.entry(g.id).or_insert(0) += made;
        self.mesh.fronds += 1;
        *rng = ev.rng;
    }
}

/// Wind offset for a vertex at time `time` (seconds). Estimated: a slow sway that grows with
/// height, plus a quicker flutter on twigs and leaves.
pub fn wind_offset(v: &Vtx, time: f32, strength: f32, freq: f32, ref_len: f32) -> V3 {
    if strength <= 0.0 {
        return V3::default();
    }
    let w = time * freq * TAU;
    let sway = v.flex * v.flex * strength * ref_len * 0.04;
    let gust = 0.6 + 0.4 * (w * 0.37).sin();
    let main = v3((w + v.flex * 0.8).sin() * gust + 0.6, 0.0, (w * 0.7 + v.flex).sin() * 0.35) * sway;
    let fl = v.flutter * strength * ref_len * 0.006;
    let flut = v3((w * 3.1 + v.phase).sin(), (w * 4.3 + v.phase * 1.7).sin() * 0.6, (w * 2.7 + v.phase).cos()) * fl;
    main + flut
}

/// Wavefront OBJ text, one group per generator.
pub fn to_obj(doc: &Doc, m: &TreeMesh) -> String {
    use std::fmt::Write;
    let mut s = format!("# {} - made with TreeCraft\n", doc.name);
    for v in &m.v {
        let _ = writeln!(s, "v {:.5} {:.5} {:.5}", v.p.x, v.p.y, v.p.z);
    }
    for v in &m.v {
        let _ = writeln!(s, "vt {:.5} {:.5}", v.uv[0], v.uv[1]);
    }
    for v in &m.v {
        let _ = writeln!(s, "vn {:.4} {:.4} {:.4}", v.n.x, v.n.y, v.n.z);
    }
    let mut cur = u32::MAX;
    for t in &m.tris {
        let gid = m.v[t[0] as usize].gen;
        if gid != cur {
            cur = gid;
            let name = doc.get(gid).map_or("mesh".to_string(), |g| g.name.replace(' ', "_"));
            let _ = writeln!(s, "g {name}");
        }
        let _ = writeln!(s, "f {0}/{0}/{0} {1}/{1}/{1} {2}/{2}/{2}", t[0] + 1, t[1] + 1, t[2] + 1);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{pv, template, TEMPLATES};

    #[test]
    fn every_template_grows_and_is_repeatable() {
        for name in TEMPLATES {
            let d = template(name);
            let a = build_lod(&d, 0.0);
            let b = build_lod(&d, 0.0);
            assert!(!a.tris.is_empty(), "{name} grew nothing");
            assert_eq!(a.tris.len(), b.tris.len(), "{name} not repeatable");
            assert!(a.tris.iter().flatten().all(|&i| (i as usize) < a.v.len()));
            assert!(a.v.iter().all(|v| v.p.x.is_finite() && v.p.y.is_finite() && v.n.x.is_finite()), "{name} has NaN");
        }
        let c = build_lod(&template("Broadleaf"), 0.0);
        assert!(c.leaves > 200 && c.branches > 50, "broadleaf too sparse: {} {}", c.branches, c.leaves);
    }

    fn one(mode: Mode) -> Gen {
        let mut g = Gen::new(1, Some(0), Kind::Branch, "b");
        g.set_mode(mode);
        g.setv("Generation:First", 0.0, 0.0);
        g.setv("Generation:Last", 1.0, 0.0);
        g
    }

    #[test]
    fn placement_rules_hold() {
        let mut parent = Inst::root(0.0);
        parent.len = 50.0;
        // Interval: frequency 2 on a 50-unit parent puts a group every 5 units (0.1 in t).
        let mut g = one(Mode::Interval);
        g.setv("Generation:Interval:Frequency", 2.0, 0.0);
        g.setv("Generation:Interval:Count", 1.0, 0.0);
        g.set("Generation:Interval:Spread", 0.0);
        let p = placements(&g, &parent, 0.0, &mut Rng::new(1));
        assert_eq!(p.len(), 11);
        assert!((p[0].t - 1.0).abs() < 1e-5 && (p[1].t - 0.9).abs() < 1e-5);
        // Absolute truncates; Proportional scales with the parent.
        let mut g = one(Mode::Absolute);
        g.setv("Generation:Absolute:Number", 5.48, 0.0);
        assert_eq!(placements(&g, &parent, 0.0, &mut Rng::new(3)).len(), 5);
        let mut g = one(Mode::Proportional);
        g.setv("Generation:Proportional:Number", 8.0, 0.0);
        assert_eq!(placements(&g, &parent, 100.0, &mut Rng::new(3)).len(), 4);
        // Phyllotaxy decussate: pairs every Internode length.
        let mut g = one(Mode::Phyllotaxy);
        g.set("Generation:Phyllotaxy:Arrangement:Style", 1.0);
        g.setv("Generation:Phyllotaxy:Nodes:Internode length", 12.5, 0.0);
        g.set("Generation:Phyllotaxy:Nodes:Spread", 0.0);
        let p = placements(&g, &parent, 0.0, &mut Rng::new(3));
        assert_eq!(p.len(), 10);
        assert!(((p[2].around - p[0].around) - 0.25).abs() < 1e-5);
        // Absolute steps: steps × per step.
        let mut g = one(Mode::AbsoluteSteps);
        g.setv("Generation:Absolute Steps:Steps", 4.0, 0.0);
        g.setv("Generation:Absolute Steps:Per Step:Number", 3.0, 0.0);
        assert_eq!(placements(&g, &parent, 0.0, &mut Rng::new(3)).len(), 12);
        // Classic with 3 steps of 2 per step.
        let mut g = one(Mode::Classic);
        g.setv("Generation:Frequency", 2.0, 0.0);
        g.set("Generation:Steps:Enabled", 1.0);
        g.setv("Generation:Steps:Number", 3.0, 0.0);
        g.set("Generation:Steps:Per step frequency", 1.0);
        assert_eq!(placements(&g, &parent, 0.0, &mut Rng::new(3)).len(), 6);
        // Parent mode sits at the start.
        let mut g = one(Mode::Parent);
        g.setv("Generation:Parent:Absolute", 3.0, 0.0);
        assert!(placements(&g, &parent, 0.0, &mut Rng::new(3)).iter().all(|s| s.t == 0.0));
    }

    #[test]
    fn scene_objects_bend_and_block() {
        use crate::model::{ObjKind, SceneObj};
        let mut s = SceneObj::new(ObjKind::Sphere, 1, 10.0);
        s.pos = [0.0, 0.0, 0.0];
        s.radius = 2.0;
        let mut f = SceneObj::new(ObjKind::Direction, 1, 10.0);
        f.dir = [1.0, 0.0, 0.0];
        assert!(apply_forces(&[f.clone()], V3::default(), V3::UP, 0.1, V3::UP, 0.0).x > 0.0, "direction force bends");
        let mut wind = SceneObj::new(ObjKind::SeasonWind, 1, 10.0);
        wind.dir = [1.0, 0.0, 0.0];
        assert_eq!(apply_forces(&[wind.clone()], V3::default(), V3::UP, 0.1, V3::UP, 0.0), V3::UP, "season wind is still in summer");
        assert!(apply_forces(&[wind], V3::default(), V3::UP, 0.1, V3::UP, 1.0).x > 0.0, "and blows in autumn");
        let mut ko = SceneObj::new(ObjKind::Knockout, 1, 10.0);
        ko.pos = [0.0, 0.0, 0.0];
        ko.radius = 1.0;
        assert!(knocked_out(&[ko], v3(0.2, 0.0, 0.0)));
        let base = template("Broadleaf");
        let mut d = base.clone();
        f.strength = 2.0;
        d.objects.push(f);
        assert!(build_lod(&d, 0.0).max.x > build_lod(&base, 0.0).max.x, "the tree leans with the force");
    }

    #[test]
    fn knockout_clip_and_break_seed_change_the_tree() {
        let base = template("Broadleaf");
        let b1 = base.gens.iter().find(|g| g.name == "Branch 1").unwrap().id;
        let count = |d: &Doc| build_lod(d, 0.0).inst_count.get(&b1).copied().unwrap_or(0);
        let mut d = base.clone();
        d.get_mut(b1).unwrap().vals.insert("Generation:Knockout".into(), pv(1.0, 0.0));
        assert!(count(&d) < count(&base), "knockout thins crowded branches");
        // Clip removes what Position pushes past Last.
        let mut d = base.clone();
        let g = d.get_mut(b1).unwrap();
        g.vals.insert("Generation:Shared:Position".into(), pv(0.3, 0.0));
        g.vals.insert("Generation:Shared:Boundaries:Clip".into(), pv(0.0, 0.0));
        let tight = count(&d);
        d.get_mut(b1).unwrap().vals.insert("Generation:Shared:Boundaries:Clip".into(), pv(1.0, 0.0));
        assert!(tight < count(&d), "clip 0 drops pushed nodes, clip 1 keeps them");
        // Break picks different branches per seed.
        let mut d = base.clone();
        d.get_mut(b1).unwrap().vals.insert("Spine:Break:Chance".into(), pv(0.5, 0.0));
        let a = build_lod(&d, 0.0).v.len();
        assert_eq!(a, build_lod(&d, 0.0).v.len(), "same seed, same breaks");
        d.get_mut(b1).unwrap().vals.insert("Spine:Break:Seed".into(), pv(123.0, 0.0));
        assert_ne!(a, build_lod(&d, 0.0).v.len(), "another seed breaks others");
        let mut c = Crowd::default();
        assert!(!c.crowded(V3::default(), 1.0) && c.crowded(v3(0.5, 0.0, 0.0), 1.0) && !c.crowded(v3(2.5, 0.0, 0.0), 1.0));
    }

    #[test]
    fn bifurcation_forks_a_straight_parent() {
        let mut d = template("Broadleaf");
        let b1 = d.gens.iter().find(|g| g.name == "Branch 1").unwrap().id;
        d.get_mut(b1).unwrap().set_mode(Mode::Bifurcation);
        let m = build_lod(&d, 0.0);
        assert!(m.per_gen.get(&b1).copied().unwrap_or(0) > 0, "the trunk splits and Branch 1 grows from the fork");
    }

    #[test]
    fn extend_parent_adds_a_tip_without_moving_the_rest() {
        let base = template("Broadleaf");
        let b1 = base.gens.iter().find(|g| g.name == "Branch 1").unwrap().id;
        let mut d = base.clone();
        d.get_mut(b1).unwrap().vals.insert("Generation:Shared:Extend parent".into(), pv(2.0, 0.0));
        let (a, b) = (build_lod(&base, 0.0), build_lod(&d, 0.0));
        assert!(b.per_gen[&b1] > a.per_gen[&b1], "Split 1 adds a tip branch");
        // The first branches of Branch 1 are built in the same places as before.
        let first = |m: &TreeMesh| m.v.iter().find(|v| v.gen == b1).map(|v| v.p).unwrap();
        assert_eq!(first(&a), first(&b), "the other children don't move");
    }

    #[test]
    fn instances_keep_their_keys_and_own_edits() {
        let base = template("Broadleaf");
        let b1 = base.gens.iter().find(|g| g.name == "Branch 1").unwrap().id;
        let a = build_lod(&base, 0.0);
        let (&key, info) = a.insts.iter().find(|(_, i)| i.gen == b1).unwrap();
        // Same key next build.
        assert!(build_lod(&base, 0.0).insts.contains_key(&key));
        // Moving one instance moves only it.
        let mut d = base.clone();
        d.get_mut(b1).unwrap().inst.insert(key, crate::model::InstEdit { offset: [0.0, 5.0, 0.0], ..Default::default() });
        let b = build_lod(&d, 0.0);
        assert!((b.insts[&key].pts[0].y - info.pts[0].y - 5.0).abs() < 1e-3, "the edited instance moved");
        let other = a.insts.iter().find(|(k, i)| i.gen == b1 && **k != key).unwrap();
        assert_eq!(b.insts[other.0].pts[0], other.1.pts[0], "the others stayed");
        // Hand-drawn: replacing it with a drawn spline keeps the branch count.
        let mut h = base.clone();
        let g = h.get_mut(b1).unwrap();
        g.replaced.insert(key);
        g.drawn.push(crate::model::DrawnSpline { parent: info.parent, pts: info.pts.iter().map(|q| [q.x, q.y + 1.0, q.z]).collect() });
        let c = build_lod(&h, 0.0);
        assert!(!c.insts.contains_key(&key), "the generated one is gone");
        assert_eq!(c.inst_count[&b1], a.inst_count[&b1], "a drawn branch took its place");
    }

    #[test]
    fn seasons_blend_and_sets_follow_the_year() {
        use crate::model::{default_seasons, season_at, MaterialSet, SetMember};
        let s = default_seasons();
        assert_eq!(season_at(&s, 0.1, 0.375), (1, 0.0), "mid-summer: green");
        assert_eq!(season_at(&s, 0.1, 0.625).1, 0.7, "mid-autumn: turned");
        let edge = season_at(&s, 0.1, 0.5).1;
        assert!(edge > 0.0 && edge < 0.7, "the border between summer and autumn blends ({edge})");
        let mut set = MaterialSet::new("S");
        set.override_season = true;
        for _ in 0..3 {
            set.members.push(SetMember { material: Some(0), weight: 1.0, season: crate::curve::Curve::flat(1.0) });
        }
        assert_eq!(set.factors(0.0), vec![1.0, 0.0, 0.0], "the first member at the start of the year");
        assert_eq!(set.factors(1.0), vec![0.0, 0.0, 1.0], "the last at the end");
        let mid = set.factors(1.0 / 3.0);
        assert!(mid[0] > 0.0 && mid[1] > 0.0, "neighbours blend between their shares ({mid:?})");
    }

    #[test]
    fn trunks_start_out_by_the_tree_radius() {
        let mut d = template("Broadleaf");
        let trunk = d.gens.iter().find(|g| g.name == "Trunk").unwrap().id;
        let base = |d: &Doc| build_lod(d, 0.0).insts.values().find(|i| i.gen == trunk).unwrap().pts[0];
        assert!(v3(base(&d).x, 0.0, base(&d).z).len() < 1e-3, "First = Last = 0 keeps the trunk in the middle");
        d.get_mut(trunk).unwrap().set("Generation:First", 0.5);
        d.get_mut(trunk).unwrap().set("Generation:Last", 0.5);
        let r = d.tree().f("Shape:Radius") * d.size_scalar();
        assert!((v3(base(&d).x, 0.0, base(&d).z).len() - r * 0.5).abs() < 1e-2, "half the tree's radius out");
    }

    #[test]
    fn leaves_pick_among_types() {
        let mut d = template("Broadleaf");
        let lid = d.gens.iter().find(|g| g.name == "Leaves").unwrap().id;
        let mut m = crate::model::Material::new("Red", [220, 30, 30], 1);
        m.var_amount = 0.0;
        d.materials.push(m);
        let red = d.materials.len() - 1;
        let g = d.get_mut(lid).unwrap();
        g.types.push(crate::model::LeafType { material: Some(red), set: None, mesh: None, weight: 1.0 });
        let mesh = build_lod(&d, 0.0);
        let reds = mesh.v.iter().filter(|v| v.gen == lid && v.mat as usize == red + 1).count();
        let all = mesh.v.iter().filter(|v| v.gen == lid).count();
        assert!(reds > 0 && reds < all, "some leaves, not all, use the added type ({reds} of {all})");
    }

    #[test]
    fn settings_change_the_tree() {
        let base = template("Broadleaf");
        let tris = |d: &Doc| build_lod(d, 0.0).tris.len();
        let n0 = tris(&base);
        let trunk = base.gens.iter().find(|g| g.name == "Trunk").unwrap().id;
        let leaves = base.gens.iter().find(|g| g.name == "Leaves").unwrap().id;
        let mut d = base.clone();
        d.get_mut(trunk).unwrap().set("Segments:Radial:Absolute", 20.0);
        assert!(tris(&d) > n0, "radial segments");
        let mut d = base.clone();
        d.get_mut(leaves).unwrap().set("Leaves:Cross", 1.0);
        assert!(tris(&d) > n0, "cross");
        let mut d = base.clone();
        d.tree_mut().set("TreeCraft:Season:Season", 1.0);
        assert!(build_lod(&d, 0.0).leaves < build_lod(&base, 0.0).leaves, "leaves fall in autumn");
        let mut d = base.clone();
        let g = d.get_mut(trunk).unwrap();
        g.vals.insert("Spine:Shape:Gravity".into(), pv(0.8, 0.0));
        assert_ne!(build_lod(&d, 0.0).max.y, build_lod(&base, 0.0).max.y, "gravity");
        assert!(build_lod(&base, 1.0).tris.len() < n0, "LOD lowers detail");
    }
}

