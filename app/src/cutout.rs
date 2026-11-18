//! Cutout mesh geometry, as the original's Cutout Editor describes it: points on the material are
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
pub fn rotate(m: &MeshAsset, p: P) -> P {
    if m.angle == 0.0 {
        return p;
    }
    let (s, c) = m.angle.to_radians().sin_cos();
    let (x, y) = (p[0] - m.pivot[0], p[1] - m.pivot[1]);
    [m.pivot[0] + x * c - y * s, m.pivot[1] + x * s + y * c]
}

/// The finished mesh for one LOD: points (rotated by Angle), triangles, and whether each
/// triangle is a back face added by Double-sided.
pub fn build(m: &MeshAsset, lod: usize) -> (Vec<P>, Vec<[usize; 3]>) {
    let mut pts: Vec<P> = m.lods[lod.min(2)].clone();
    if pts.len() < 3 {
        pts = MeshAsset::leaf_card().lods[0].clone();
    }
    let mut tris = delaunay(&pts);
    // Snap threshold: drop slivers thinner than this share of their longest edge.
    if m.snap > 0.0 {
        tris.retain(|t| {
            let (a, b, c) = (pts[t[0]], pts[t[1]], pts[t[2]]);
            let area2 = ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs();
            let longest = [(a, b), (b, c), (c, a)].iter().map(|(p, q)| (p[0] - q[0]).hypot(p[1] - q[1])).fold(0.0f32, f32::max);
            area2 / (longest * longest).max(1e-9) > m.snap * 0.2
        });
    }
    let mask = &m.mask[lod.min(2)];
    tris.retain(|t| {
        let c = [(pts[t[0]][0] + pts[t[1]][0] + pts[t[2]][0]) / 3.0, (pts[t[0]][1] + pts[t[1]][1] + pts[t[2]][1]) / 3.0];
        !masked(mask, c)
    });
    let times = (m.tess_amount * 3.0).round() as usize + if m.tess_style == 1 && m.tess_amount > 0.0 { 1 } else { 0 };
    if times > 0 {
        tris = subdivide(&mut pts, tris, times.min(4));
    }
    if m.refine > 0.0 {
        tris = refine_edges(&mut pts, tris, m.refine);
    }
    let pts: Vec<P> = pts.iter().map(|&p| rotate(m, p)).collect();
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

/// Generate a fresh point set (the original's Generate menu). Without a texture to trace, the
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
