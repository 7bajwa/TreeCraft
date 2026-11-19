//! Cutout mesh geometry, as the Cutout Editor describes it: points on the material are
//! joined into triangles automatically (Delaunay), masked triangles are dropped, optional
//! tessellation adds triangles, and Generate makes a fresh point set.

use crate::model::MeshAsset;

pub type P = [f32; 2];

fn circum(a: P, b: P, c: P) -> Option<(P, f32)> {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-12 {
        return None;
    }
    let (a2, b2, c2) = (a[0] * a[0] + a[1] * a[1], b[0] * b[0] + b[1] * b[1], c[0] * c[0] + c[1] * c[1]);
    let ux = (a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d;
    let uy = (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d;
    Some(([ux, uy], (ux - a[0]).powi(2) + (uy - a[1]).powi(2)))
}

/// Delaunay triangulation (Bowyer-Watson) of a point set; indices into `pts`.
pub fn delaunay(pts: &[P]) -> Vec<[usize; 3]> {
    let n = pts.len();
    if n < 3 {
        return vec![];
    }
    let mut all: Vec<P> = pts.to_vec();
    // Tiny jitter keeps exactly-cocircular grids from producing slivers.
    for (i, p) in all.iter_mut().enumerate() {
        p[0] += (i as f32 * 0.618_034).fract() * 1e-5;
        p[1] += (i as f32 * 0.414_213).fract() * 1e-5;
    }
    all.extend([[-100.0, -100.0], [100.0, -100.0], [0.0, 100.0]]);
    let mut tris: Vec<[usize; 3]> = vec![[n, n + 1, n + 2]];
    for i in 0..n {
        let p = all[i];
        let mut bad = vec![];
        tris.retain(|t| {
            let inside = circum(all[t[0]], all[t[1]], all[t[2]]).is_some_and(|(c, r2)| (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) < r2);
            if inside {
                bad.push(*t);
            }
            !inside
        });
        let mut edges: Vec<[usize; 2]> = vec![];
        for t in &bad {
            for e in [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]] {
                if let Some(k) = edges.iter().position(|f| (f[0] == e[1] && f[1] == e[0]) || *f == e) {
                    edges.swap_remove(k);
                } else {
                    edges.push(e);
                }
            }
        }
        for e in edges {
            tris.push([e[0], e[1], i]);
        }
    }
    tris.retain(|t| t.iter().all(|&k| k < n));
    // Counter-clockwise winding.
    for t in &mut tris {
        let (a, b, c) = (pts[t[0]], pts[t[1]], pts[t[2]]);
        if (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) < 0.0 {
            t.swap(1, 2);
        }
    }
    tris
}

fn masked(mask: &[[f32; 3]], p: P) -> bool {
    mask.iter().any(|m| (p[0] - m[0]).powi(2) + (p[1] - m[1]).powi(2) < m[2] * m[2])
}

/// Split each triangle `times` times at edge midpoints (shared midpoints are reused).
fn subdivide(pts: &mut Vec<P>, tris: Vec<[usize; 3]>, times: usize) -> Vec<[usize; 3]> {
    let mut tris = tris;
    for _ in 0..times {
        let mut mids = std::collections::HashMap::new();
        let mut out = vec![];
        let mut mid = |a: usize, b: usize, pts: &mut Vec<P>| -> usize {
            *mids.entry((a.min(b), a.max(b))).or_insert_with(|| {
                pts.push([(pts[a][0] + pts[b][0]) * 0.5, (pts[a][1] + pts[b][1]) * 0.5]);
                pts.len() - 1
            })
        };
        for t in tris {
            let (ab, bc, ca) = (mid(t[0], t[1], pts), mid(t[1], t[2], pts), mid(t[2], t[0], pts));
            out.extend([[t[0], ab, ca], [ab, t[1], bc], [ca, bc, t[2]], [ab, bc, ca]]);
        }
        tris = out;
    }
    tris
}

/// Rotate a point by the mesh angle around the pivot.
pub fn rotate(angle: f32, pivot: P, p: P) -> P {
    if angle == 0.0 {
        return p;
    }
    let (s, c) = angle.to_radians().sin_cos();
    let (x, y) = (p[0] - pivot[0], p[1] - pivot[1]);
    [pivot[0] + x * c - y * s, pivot[1] + x * s + y * c]
}

/// The finished mesh for one LOD: points (rotated by Angle), triangles, and whether each
/// triangle is a back face added by Double-sided.
pub fn build(m: &MeshAsset, lod: usize) -> (Vec<P>, Vec<[usize; 3]>) {
    let mut pts: Vec<P> = m.lods[lod.min(2)].clone();
    if pts.len() < 3 {
        pts = MeshAsset::leaf_card().lods[0].clone();
    }
    let cfg = m.cfg(lod);
    let mut tris = delaunay(&pts);
    // Snap threshold: drop slivers thinner than this share of their longest edge.
    if cfg.snap > 0.0 {
        tris.retain(|t| {
            let (a, b, c) = (pts[t[0]], pts[t[1]], pts[t[2]]);
            let area2 = ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs();
            let longest = [(a, b), (b, c), (c, a)].iter().map(|(p, q)| (p[0] - q[0]).hypot(p[1] - q[1])).fold(0.0f32, f32::max);
            area2 / (longest * longest).max(1e-9) > cfg.snap * 0.2
        });
    }
    let mask = &m.mask[lod.min(2)];
    tris.retain(|t| {
        let c = [(pts[t[0]][0] + pts[t[1]][0] + pts[t[2]][0]) / 3.0, (pts[t[0]][1] + pts[t[1]][1] + pts[t[2]][1]) / 3.0];
        !masked(mask, c)
    });
    let times = (cfg.tess_amount * 3.0).round() as usize + if cfg.tess_style == 1 && cfg.tess_amount > 0.0 { 1 } else { 0 };
    if times > 0 {
        tris = subdivide(&mut pts, tris, times.min(4));
    }
    if cfg.refine > 0.0 {
        tris = refine_edges(&mut pts, tris, cfg.refine);
    }
    let pts: Vec<P> = pts.iter().map(|&p| rotate(cfg.angle, cfg.pivot, p)).collect();
    (pts, tris)
}

/// Split triangles touching the outline so its edges get more points.
fn refine_edges(pts: &mut Vec<P>, tris: Vec<[usize; 3]>, amount: f32) -> Vec<[usize; 3]> {
    let mut count = std::collections::HashMap::new();
    for t in &tris {
        for e in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *count.entry((e.0.min(e.1), e.0.max(e.1))).or_insert(0) += 1;
        }
    }
    let rounds = (amount * 2.0).ceil() as usize;
    let mut tris = tris;
    for _ in 0..rounds {
        let mut out = vec![];
        for t in tris {
            let edge = [(t[0], t[1], t[2]), (t[1], t[2], t[0]), (t[2], t[0], t[1])].into_iter().find(|e| count.get(&(e.0.min(e.1), e.0.max(e.1))) == Some(&1));
            match edge {
                Some((a, b, c)) => {
                    pts.push([(pts[a][0] + pts[b][0]) * 0.5, (pts[a][1] + pts[b][1]) * 0.5]);
                    let m = pts.len() - 1;
                    count.remove(&(a.min(b), a.max(b)));
                    count.insert((a.min(m), a.max(m)), 1);
                    count.insert((m.min(b), m.max(b)), 1);
                    out.extend([[a, m, c], [m, b, c]]);
                }
                None => out.push(t),
            }
        }
        tris = out;
    }
    tris
}

/// Width of the shape at height y: the outline's left and right x (convex hull of the points).
pub fn span_at(pts: &[P], tris: &[[usize; 3]], y: f32) -> Option<(f32, f32)> {
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for t in tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (p, q) = (pts[a], pts[b]);
            if (p[1] - y) * (q[1] - y) <= 0.0 && (p[1] - q[1]).abs() > 1e-9 {
                let x = p[0] + (q[0] - p[0]) * (y - p[1]) / (q[1] - p[1]);
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// Generate a fresh point set (the Generate menu). Without a texture to trace, the
/// outline is a leaf shape: `style` 0 = slices with a centre line, 1 = slices, 2 = adaptive edge.
pub fn generate(style: usize, density: usize) -> Vec<P> {
    let n = density.clamp(4, 200);
    let half = |y: f32| 0.46 * (std::f32::consts::PI * y.clamp(0.0, 1.0)).sin().powf(0.8) * (1.0 - 0.25 * y);
    let mut pts = vec![[0.5, 0.0], [0.5, 1.0]];
    match style {
        2 => {
            let k = n.max(6);
            for i in 1..k / 2 {
                let y = i as f32 / (k / 2) as f32;
                pts.push([0.5 - half(y), y]);
                pts.push([0.5 + half(y), y]);
            }
        }
        _ => {
            let slices = ((n as f32) / if style == 0 { 3.0 } else { 2.0 }).round().max(2.0) as usize;
            for i in 1..slices {
                let y = i as f32 / slices as f32;
                pts.push([0.5 - half(y), y]);
                pts.push([0.5 + half(y), y]);
                if style == 0 {
                    pts.push([0.5, y]);
                }
            }
        }
    }
    pts
}

/// Trace a cutout from a picture's opacity (`alpha` row-major, y down, `w`×`h`): about `density`
/// points along the edge of the opaque area plus a few inside, and mask spots on the triangles
/// that would cover see-through parts. None when nothing is opaque.
pub fn from_alpha(alpha: &[u8], w: usize, h: usize, density: usize) -> Option<(Vec<P>, Vec<[f32; 3]>)> {
    const G: usize = 64;
    if w == 0 || h == 0 || alpha.len() < w * h {
        return None;
    }
    let cell = |g: usize, n: usize| (g * n / G, ((g + 1) * n / G).max(g * n / G + 1).min(n));
    let grid: Vec<bool> = (0..G * G)
        .map(|i| {
            let ((x0, x1), (y0, y1)) = (cell(i % G, w), cell(i / G, h));
            (y0..y1).any(|y| (x0..x1).any(|x| alpha[y * w + x] >= 128))
        })
        .collect();
    let solid = |x: isize, y: isize| x >= 0 && y >= 0 && (x as usize) < G && (y as usize) < G && grid[y as usize * G + x as usize];
    let card = |x: f32, y: f32| -> P { [(x / G as f32).clamp(0.0, 1.0), (1.0 - y / G as f32).clamp(0.0, 1.0)] };
    let (mut edge, mut inner) = (vec![], vec![]);
    for gy in 0..G as isize {
        for gx in 0..G as isize {
            if !solid(gx, gy) {
                continue;
            }
            // Edge cells move half a cell toward their see-through neighbours.
            let (mut ox, mut oy) = (0.0, 0.0);
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                if !solid(gx + dx, gy + dy) {
                    ox += dx as f32 * 0.5;
                    oy += dy as f32 * 0.5;
                }
            }
            let q = card(gx as f32 + 0.5 + ox, gy as f32 + 0.5 + oy);
            if ox != 0.0 || oy != 0.0 { edge.push(q) } else { inner.push(q) }
        }
    }
    if edge.is_empty() {
        return None;
    }
    let d2 = |a: P, b: P| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2);
    // Farthest-point picks spread the points evenly, starting at the stem (lowest point).
    let spread = |cands: &[P], chosen: &mut Vec<P>, n: usize, min_d2: f32| {
        let mut best: Vec<f32> = cands.iter().map(|&c| chosen.iter().map(|&q| d2(c, q)).fold(f32::MAX, f32::min)).collect();
        for _ in 0..n {
            let Some((i, &d)) = best.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)) else { break };
            if d < min_d2 {
                break;
            }
            let c = cands[i];
            chosen.push(c);
            for (b, &q) in best.iter_mut().zip(cands) {
                *b = b.min(d2(q, c));
            }
        }
    };
    let n = density.clamp(4, 200);
    let start = *edge.iter().min_by(|a, b| a[1].total_cmp(&b[1])).unwrap();
    let mut pts = vec![start];
    spread(&edge, &mut pts, n - 1, 0.0);
    let gap = pts.windows(2).map(|w| d2(w[0], w[1]).sqrt()).fold(0.0f32, f32::max).max(0.05);
    spread(&inner, &mut pts, n / 3, (gap * 0.8).powi(2));
    let mask = delaunay(&pts)
        .iter()
        .map(|t| [(pts[t[0]][0] + pts[t[1]][0] + pts[t[2]][0]) / 3.0, (pts[t[0]][1] + pts[t[1]][1] + pts[t[2]][1]) / 3.0])
        .filter(|c| !solid((c[0] * G as f32) as isize, ((1.0 - c[1]) * G as f32) as isize))
        .map(|c| [c[0], c[1], 0.004])
        .collect();
    Some((pts, mask))
}

/// A point of the mesh (card points measured from the pivot, z = 0 on the card) in the leaf's
/// own frame (x side, y growth, z out of the card), following the mesh's Orient, Flip and
/// Growth settings. Orient names the axes it was drawn in: Y-Up RH is the card standing up;
/// Z-Up lays it flat; left-handed mirrors depth.
pub fn axes(orient: usize, flip: usize, growth: usize, p: [f32; 3]) -> [f32; 3] {
    let [mut x, mut y, mut z] = p;
    match flip {
        1 => x = -x,
        2 => y = -y,
        3 => z = -z,
        _ => {}
    }
    let (x, y, z) = match orient {
        0 => (x, z, -y),
        2 => (x, y, -z),
        3 => (x, z, y),
        _ => (x, y, z),
    };
    // The axis the leaf was drawn growing along is turned to +Y.
    let (x, y, z) = match growth {
        1 => (-y, x, z),
        2 => (y, -x, z),
        3 => (x, z, -y),
        4 => (x, -z, y),
        _ => (x, y, z),
    };
    [x, y, z]
}

/// Which way the card's front faces in the leaf's frame (Flip normals turns it around).
pub fn front(orient: usize, flip: usize, growth: usize, flip_normals: bool) -> [f32; 3] {
    let n = axes(orient, flip, growth, [0.0, 0.0, 1.0]);
    if flip_normals { n.map(|v| -v) } else { n }
}

/// Mirror: make the left side match the right (points near the middle snap to it).
pub fn mirror(pts: &[P]) -> Vec<P> {
    let mut out: Vec<P> = vec![];
    for &p in pts {
        if (p[0] - 0.5).abs() < 0.02 {
            out.push([0.5, p[1]]);
        } else if p[0] > 0.5 {
            out.push(p);
            out.push([1.0 - p[0], p[1]]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn axes_follow_flip_and_growth() {
        assert_eq!(axes(1, 0, 0, [0.2, 1.0, 0.0]), [0.2, 1.0, 0.0], "Y-Up RH keeps the card standing");
        assert_eq!(axes(1, 1, 0, [0.2, 1.0, 0.0]), [-0.2, 1.0, 0.0], "Flip -X mirrors the card");
        assert_eq!(axes(1, 0, 1, [1.0, 0.0, 0.0]), [-0.0, 1.0, 0.0], "a leaf drawn growing along +X is turned upright");
        assert_eq!(axes(0, 0, 0, [0.0, 1.0, 0.0]), [0.0, 0.0, -1.0], "Z-Up lays the card flat");
        assert_eq!(front(1, 0, 0, false), [0.0, 0.0, 1.0]);
        assert_eq!(front(1, 0, 0, true), [-0.0, -0.0, -1.0], "Flip normals turns the front around");
    }

    #[test]
    fn traces_opacity_and_masks_holes() {
        // A ring: opaque between radius 0.2 and 0.45 of a 128-pixel picture.
        let n = 128;
        let alpha: Vec<u8> = (0..n * n)
            .map(|i| {
                let (x, y) = ((i % n) as f32 / n as f32 - 0.5, (i / n) as f32 / n as f32 - 0.5);
                let r = x.hypot(y);
                if (0.2..0.45).contains(&r) { 255 } else { 0 }
            })
            .collect();
        let (pts, mask) = from_alpha(&alpha, n, n, 24).unwrap();
        assert!(pts.len() >= 24);
        assert!(pts.iter().all(|p| (p[0] - 0.5).hypot(p[1] - 0.5) < 0.5), "points stay on the picture");
        assert!(!mask.is_empty(), "the hole gets masked");
        assert!(from_alpha(&vec![0; n * n], n, n, 24).is_none());
    }

    #[test]
    fn square_makes_two_triangles_and_mask_removes() {
        let pts = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        assert_eq!(delaunay(&pts).len(), 2);
        let mut m = MeshAsset::leaf_card();
        m.lods[0] = pts;
        assert_eq!(build(&m, 0).1.len(), 2);
        let (p, t) = build(&m, 0);
        let c = [(p[t[0][0]][0] + p[t[0][1]][0] + p[t[0][2]][0]) / 3.0, (p[t[0][0]][1] + p[t[0][1]][1] + p[t[0][2]][1]) / 3.0];
        m.mask[0].push([c[0], c[1], 0.1]);
        assert_eq!(build(&m, 0).1.len(), 1);
        m.mask[0].clear();
        m.tess_amount = 0.34;
        assert_eq!(build(&m, 0).1.len(), 8);
    }
    #[test]
    fn leaf_card_is_triangulated_and_spans() {
        let m = MeshAsset::leaf_card();
        let (p, t) = build(&m, 0);
        assert!(t.len() >= 5);
        let (lo, hi) = span_at(&p, &t, 0.55).unwrap();
        assert!(lo < 0.1 && hi > 0.9);
        assert!(generate(0, 20).len() > 10 && mirror(&generate(1, 10)).len() >= 6);
    }
}
