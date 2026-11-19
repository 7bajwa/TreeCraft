//! The TreeCraft logo: a cream tree (three round crowns on a forked trunk) on a rounded square
//! that runs from leaf green to deep teal. Drawn from shapes, so it's sharp at any size; the
//! build script makes the Windows icon from it and the app uses it as the window icon.

/// The logo at `size`×`size`: RGBA bytes, row by row from the top (4×4 samples per pixel).
pub fn logo_rgba(size: u32) -> Vec<u8> {
    let n = size.max(1) as usize;
    let mut out = vec![0u8; n * n * 4];
    const SS: usize = 4;
    for py in 0..n {
        for px in 0..n {
            let mut acc = [0.0f32; 4]; // premultiplied
            for sy in 0..SS {
                for sx in 0..SS {
                    let u = (px as f32 + (sx as f32 + 0.5) / SS as f32) / n as f32;
                    let v = (py as f32 + (sy as f32 + 0.5) / SS as f32) / n as f32;
                    let c = sample(u, v);
                    for k in 0..3 {
                        acc[k] += c[k] * c[3];
                    }
                    acc[3] += c[3];
                }
            }
            let k = (SS * SS) as f32;
            let a = acc[3] / k;
            let i = (py * n + px) * 4;
            for c in 0..3 {
                out[i + c] = if a > 0.0 { ((acc[c] / k / a).clamp(0.0, 1.0) * 255.0).round() as u8 } else { 0 };
            }
            out[i + 3] = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    out
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Distance from p to the segment a–b, and how far along it (0..1) the nearest point is.
fn seg(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> (f32, f32) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    let (qx, qy) = (a[0] + dx * t - p[0], a[1] + dy * t - p[1]);
    ((qx * qx + qy * qy).sqrt(), t)
}

/// The colour (RGB 0..1) and coverage at a point of the unit square (y down).
fn sample(u: f32, v: f32) -> [f32; 4] {
    // Rounded square background.
    let (cx, cy, half, r) = (0.5, 0.5, 0.47, 0.2);
    let (qx, qy) = ((u - cx).abs() - (half - r), (v - cy).abs() - (half - r));
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
    if outside > 0.0 {
        return [0.0; 4];
    }
    let top = [0.40, 0.72, 0.30];
    let bottom = [0.07, 0.36, 0.42];
    let mut col = mix(top, bottom, v.clamp(0.0, 1.0));
    // A soft light from the top left.
    let glow = (1.0 - ((u - 0.3).powi(2) + (v - 0.2).powi(2)).sqrt() * 1.6).clamp(0.0, 1.0) * 0.12;
    col = mix(col, [1.0; 3], glow);
    // Ground line.
    let (g, _) = seg([u, v], [0.24, 0.84], [0.76, 0.84]);
    if g < 0.018 {
        col = [0.95, 0.90, 0.80];
    }
    let cream = [0.97, 0.93, 0.85];
    let bark = [0.86, 0.76, 0.60];
    // Crowns: three overlapping circles, with a darker rim so they read as separate.
    let crowns = [([0.5, 0.33], 0.19), ([0.32, 0.45], 0.135), ([0.68, 0.45], 0.135)];
    let mut in_crown = false;
    for (c, rr) in crowns {
        let d = ((u - c[0]).powi(2) + (v - c[1]).powi(2)).sqrt();
        if d < rr {
            in_crown = true;
            col = if d > rr - 0.018 { mix(cream, bottom, 0.25) } else { cream };
        }
    }
    // Trunk (tapered) and two branches, drawn over the crowns' lower edge.
    let (dt, t) = seg([u, v], [0.5, 0.84], [0.5, 0.5]);
    let trunk_w = 0.055 + (0.03 - 0.055) * t;
    let (b1, _) = seg([u, v], [0.5, 0.64], [0.36, 0.5]);
    let (b2, _) = seg([u, v], [0.5, 0.6], [0.65, 0.47]);
    if dt < trunk_w || b1 < 0.022 || b2 < 0.022 {
        col = if in_crown { mix(bark, bottom, 0.15) } else { bark };
    }
    // A small leaf vein in the big crown.
    let (vein, _) = seg([u, v], [0.5, 0.42], [0.5, 0.24]);
    if vein < 0.008 && in_crown {
        col = mix(cream, bottom, 0.35);
    }
    [col[0], col[1], col[2], 1.0]
}
