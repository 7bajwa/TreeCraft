//! Procedural map pictures for the Edit Map dialog: gradients and noises, coloured through a
//! colour ramp (like a gradient map), plus presets.

use crate::math::hash;
use serde::{Deserialize, Serialize};

/// A colour ramp: stops at 0..1, sorted by position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ramp {
    pub stops: Vec<(f32, [u8; 3])>,
}

impl Default for Ramp {
    fn default() -> Self {
        Ramp { stops: vec![(0.0, [0; 3]), (1.0, [255; 3])] }
    }
}

impl Ramp {
    pub fn new(stops: &[(f32, [u8; 3])]) -> Ramp {
        let mut r = Ramp { stops: stops.to_vec() };
        r.sort();
        r
    }
    pub fn sort(&mut self) {
        self.stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    /// The colour at t (clamped), blended between the two stops around it.
    pub fn eval(&self, t: f32) -> [u8; 3] {
        let t = t.clamp(0.0, 1.0);
        let s = &self.stops;
        match s.len() {
            0 => [(t * 255.0) as u8; 3],
            1 => s[0].1,
            _ => {
                if t <= s[0].0 {
                    return s[0].1;
                }
                for w in s.windows(2) {
                    if t <= w[1].0 {
                        let f = ((t - w[0].0) / (w[1].0 - w[0].0).max(1e-6)).clamp(0.0, 1.0);
                        return [0, 1, 2].map(|i| (w[0].1[i] as f32 + (w[1].1[i] as f32 - w[0].1[i] as f32) * f).round() as u8);
                    }
                }
                s[s.len() - 1].1
            }
        }
    }
}

pub const SOURCES: [&str; 4] = ["File", "Color", "Gradient", "Noise"];
pub const GRADIENTS: [&str; 5] = ["Linear", "Radial", "Angular", "Diamond", "Reflected"];
pub const NOISES: [&str; 7] = ["Cells", "Voronoi cracks", "Perlin", "Clouds", "Ridged", "Marble", "Wood"];

/// Where a map's picture comes from, and how a generated one is made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapGen {
    /// 0 file, 1 value only, 2 gradient, 3 noise.
    pub source: usize,
    pub gradient: usize,
    /// Gradient start and end on the picture (0..1, y down).
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    pub noise: usize,
    pub scale: f32,
    /// Octaves for Clouds, Ridged, Marble and Wood.
    pub detail: usize,
    pub seed: u32,
    pub ramp: Ramp,
    /// Noise pan: the pattern slides by this much (picture units).
    #[serde(default)]
    pub offset: [f32; 2],
}

impl Default for MapGen {
    fn default() -> Self {
        MapGen { source: 0, gradient: 0, p0: [0.5, 1.0], p1: [0.5, 0.0], noise: 0, scale: 6.0, detail: 4, seed: 1, ramp: Ramp::default(), offset: [0.0; 2] }
    }
}

impl MapGen {
    /// The value (0..1) at picture point (x, y), before the ramp.
    pub fn value(&self, x: f32, y: f32) -> f32 {
        if self.source == 2 {
            let (a, b) = (self.p0, self.p1);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len2 = (dx * dx + dy * dy).max(1e-6);
            let (px, py) = (x - a[0], y - a[1]);
            let along = (px * dx + py * dy) / len2;
            return match self.gradient {
                1 => (px * px + py * py).sqrt() / len2.sqrt(),
                2 => ((py.atan2(px) - dy.atan2(dx)) / std::f32::consts::TAU).rem_euclid(1.0),
                3 => {
                    let across = (px * -dy + py * dx) / len2;
                    along.abs() + across.abs()
                }
                4 => 1.0 - (along * 2.0 - 1.0).abs(),
                _ => along,
            }
            .clamp(0.0, 1.0);
        }
        let s = self.scale.max(0.1);
        let seed = self.seed as u64;
        let (x, y) = ((x - self.offset[0]) * s, (y - self.offset[1]) * s);
        match self.noise {
            0 => (worley(seed, x, y).0 * 1.2).clamp(0.0, 1.0),
            1 => {
                let (f1, f2) = worley(seed, x, y);
                ((f2 - f1) * 3.0).clamp(0.0, 1.0)
            }
            2 => perlin(seed, x, y) * 0.5 + 0.5,
            3 => fbm(seed, x, y, self.detail) * 0.5 + 0.5,
            4 => {
                let mut v = 0.0;
                let (mut amp, mut f) = (0.5, 1.0);
                for o in 0..self.detail.max(1) {
                    v += (1.0 - perlin(seed + o as u64, x * f, y * f).abs()).powi(2) * amp;
                    amp *= 0.5;
                    f *= 2.0;
                }
                v.clamp(0.0, 1.0)
            }
            5 => ((x + fbm(seed, x, y, self.detail) * 4.0).sin() * 0.5 + 0.5).clamp(0.0, 1.0),
            _ => {
                let r = ((x - s * 0.5).powi(2) + (y - s * 0.5).powi(2)).sqrt();
                (r * 2.0 + fbm(seed, x, y, self.detail) * 1.5).rem_euclid(1.0)
            }
        }
    }
    /// The generated picture, `size` pixels square.
    pub fn image(&self, size: u32) -> image::RgbaImage {
        image::RgbaImage::from_fn(size, size, |px, py| {
            let c = self.ramp.eval(self.value((px as f32 + 0.5) / size as f32, (py as f32 + 0.5) / size as f32));
            image::Rgba([c[0], c[1], c[2], 255])
        })
    }
}

fn rand2(seed: u64, ix: i32, iy: i32) -> (f32, f32) {
    let h = hash(seed, ((ix as u32 as u64) << 32) | iy as u32 as u64);
    ((h >> 40) as f32 / (1u64 << 24) as f32, ((h >> 16) & 0xff_ffff) as f32 / (1u64 << 24) as f32)
}

/// Distances to the nearest and second-nearest cell point.
fn worley(seed: u64, x: f32, y: f32) -> (f32, f32) {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (mut f1, mut f2) = (9.0f32, 9.0f32);
    for oy in -1..=1 {
        for ox in -1..=1 {
            let (cx, cy) = (ix + ox, iy + oy);
            let (rx, ry) = rand2(seed, cx, cy);
            let d = ((cx as f32 + rx - x).powi(2) + (cy as f32 + ry - y).powi(2)).sqrt();
            if d < f1 {
                f2 = f1;
                f1 = d;
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    (f1, f2)
}

/// Gradient noise in about -1..1.
fn perlin(seed: u64, x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - ix as f32, y - iy as f32);
    let grad = |cx: i32, cy: i32, dx: f32, dy: f32| {
        let a = rand2(seed ^ 0x9e37, cx, cy).0 * std::f32::consts::TAU;
        a.cos() * dx + a.sin() * dy
    };
    let s = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let (u, v) = (s(fx), s(fy));
    let n00 = grad(ix, iy, fx, fy);
    let n10 = grad(ix + 1, iy, fx - 1.0, fy);
    let n01 = grad(ix, iy + 1, fx, fy - 1.0);
    let n11 = grad(ix + 1, iy + 1, fx - 1.0, fy - 1.0);
    let a = n00 + (n10 - n00) * u;
    let b = n01 + (n11 - n01) * u;
    ((a + (b - a) * v) * 1.4).clamp(-1.0, 1.0)
}

fn fbm(seed: u64, x: f32, y: f32, octaves: usize) -> f32 {
    let (mut v, mut amp, mut f, mut total) = (0.0, 0.5, 1.0, 0.0);
    for o in 0..octaves.max(1) {
        v += perlin(seed + o as u64 * 31, x * f, y * f) * amp;
        total += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    v / total
}

/// A displacement picture read for sampling: heights 0..1, `w`×`h`, wrapping.
pub struct DispMap {
    pub w: usize,
    pub h: usize,
    pub v: Vec<f32>,
}

impl DispMap {
    /// The height at (x, y) in picture units (repeats), blended between pixels.
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        let (fx, fy) = (x.rem_euclid(1.0) * self.w as f32 - 0.5, y.rem_euclid(1.0) * self.h as f32 - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let at = |x: f32, y: f32| self.v[(y as i64).rem_euclid(self.h as i64) as usize * self.w + (x as i64).rem_euclid(self.w as i64) as usize];
        let a = at(x0, y0) + (at(x0 + 1.0, y0) - at(x0, y0)) * tx;
        let b = at(x0, y0 + 1.0) + (at(x0 + 1.0, y0 + 1.0) - at(x0, y0 + 1.0)) * tx;
        a + (b - a) * ty
    }
    pub fn image(&self) -> image::RgbaImage {
        image::RgbaImage::from_fn(self.w as u32, self.h as u32, |x, y| {
            let g = (self.v[y as usize * self.w + x as usize].clamp(0.0, 1.0) * 255.0) as u8;
            image::Rgba([g, g, g, 255])
        })
    }
}

type DispCache = std::sync::Mutex<std::collections::HashMap<String, Option<std::sync::Arc<DispMap>>>>;

fn disp_cache() -> &'static DispCache {
    static CACHE: std::sync::OnceLock<DispCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Forget the read displacement pictures (after a file changed on disk).
pub fn clear_disp_cache() {
    if let Ok(mut c) = disp_cache().lock() {
        c.clear();
    }
}

/// A displacement asset's heights: its picture's channel, brightness and contrast, smoothed and
/// normalized as set. Read once per settings and kept.
pub fn disp_heights(a: &crate::model::DispAsset) -> Option<std::sync::Arc<DispMap>> {
    use std::sync::Arc;
    let key = serde_json::to_string(&(&a.file, a.channel, a.smoothing, a.brightness, a.contrast, a.normalize, &a.gen)).ok()?;
    let cache = disp_cache();
    if let Some(m) = cache.lock().ok()?.get(&key) {
        return m.clone();
    }
    let made = (|| {
        // A gradient or noise: its values (before the colour ramp) are the heights.
        let img = if a.gen.source >= 2 {
            let n = 256u32;
            image::RgbaImage::from_fn(n, n, |x, y| {
                let g = (a.gen.value((x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32).clamp(0.0, 1.0) * 255.0) as u8;
                image::Rgba([g, g, g, 255])
            })
        } else {
            image::open(a.file.as_ref()?).ok()?.to_rgba8()
        };
        let img = if img.width() > 256 || img.height() > 256 { image::imageops::resize(&img, 256, 256, image::imageops::FilterType::Triangle) } else { img };
        let (w, h) = (img.width() as usize, img.height() as usize);
        let mut v: Vec<f32> = img
            .pixels()
            .map(|p| {
                let x = match a.channel {
                    1..=4 => p[a.channel - 1] as f32 / 255.0,
                    _ => (p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114) / 255.0,
                };
                ((x + a.brightness - 0.5) * (1.0 + a.contrast) + 0.5).clamp(0.0, 1.0)
            })
            .collect();
        // Smoothing: a few passes of a 3×3 blur (wrapping).
        for _ in 0..(a.smoothing.clamp(0.0, 1.0) * 8.0).round() as usize {
            let src = v.clone();
            for y in 0..h {
                for x in 0..w {
                    let mut sum = 0.0;
                    for (dx, dy) in [(-1i64, -1i64), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                        sum += src[(y as i64 + dy).rem_euclid(h as i64) as usize * w + (x as i64 + dx).rem_euclid(w as i64) as usize];
                    }
                    v[y * w + x] = sum / 9.0;
                }
            }
        }
        if a.normalize {
            let (lo, hi) = v.iter().fold((1.0f32, 0.0f32), |(l, h), &x| (l.min(x), h.max(x)));
            if hi - lo > 1e-4 {
                v.iter_mut().for_each(|x| *x = (*x - lo) / (hi - lo));
            }
        }
        Some(Arc::new(DispMap { w, h, v }))
    })();
    cache.lock().ok()?.insert(key, made.clone());
    made
}

/// Ready-made gradients and noises.
pub fn presets() -> Vec<(&'static str, MapGen)> {
    let g = |gradient: usize, stops: &[(f32, [u8; 3])]| MapGen { source: 2, gradient, ramp: Ramp::new(stops), ..MapGen::default() };
    let n = |noise: usize, scale: f32, detail: usize, stops: &[(f32, [u8; 3])]| MapGen { source: 3, noise, scale, detail, ramp: Ramp::new(stops), ..MapGen::default() };
    vec![
        ("Gradient: black to white", g(0, &[(0.0, [0; 3]), (1.0, [255; 3])])),
        ("Gradient: leaf green", g(0, &[(0.0, [38, 70, 22]), (0.6, [88, 140, 48]), (1.0, [170, 205, 90])])),
        ("Gradient: autumn", g(0, &[(0.0, [120, 40, 20]), (0.5, [215, 110, 30]), (1.0, [240, 200, 70])])),
        ("Gradient: radial glow", g(1, &[(0.0, [255; 3]), (1.0, [0; 3])])),
        ("Gradient: sunset", g(0, &[(0.0, [40, 30, 90]), (0.5, [220, 90, 70]), (1.0, [250, 210, 120])])),
        ("Noise: bark cells", n(0, 8.0, 4, &[(0.0, [40, 28, 20]), (0.6, [96, 72, 52]), (1.0, [140, 112, 86])])),
        ("Noise: cracked bark", n(1, 7.0, 4, &[(0.0, [30, 20, 14]), (0.25, [80, 60, 44]), (1.0, [120, 96, 74])])),
        ("Noise: clouds", n(3, 4.0, 5, &[(0.0, [0; 3]), (1.0, [255; 3])])),
        ("Noise: lichen", n(3, 10.0, 4, &[(0.0, [90, 80, 60]), (0.55, [120, 110, 85]), (0.62, [150, 170, 90]), (1.0, [190, 200, 140])])),
        ("Noise: ridged", n(4, 5.0, 5, &[(0.0, [0; 3]), (1.0, [255; 3])])),
        ("Noise: marble", n(5, 4.0, 5, &[(0.0, [235, 232, 225]), (0.8, [180, 175, 170]), (1.0, [90, 90, 95])])),
        ("Noise: wood rings", n(6, 6.0, 3, &[(0.0, [150, 105, 60]), (0.5, [190, 140, 85]), (1.0, [120, 80, 45])])),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ramp_blends_between_stops() {
        let r = Ramp::new(&[(1.0, [255, 0, 0]), (0.0, [0, 0, 255])]);
        assert_eq!(r.eval(0.0), [0, 0, 255]);
        assert_eq!(r.eval(1.0), [255, 0, 0]);
        assert_eq!(r.eval(0.5), [128, 0, 128]);
    }
    #[test]
    fn generators_fill_the_range() {
        for (name, g) in presets() {
            let img = g.image(32);
            let lum: Vec<u32> = img.pixels().map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32).collect();
            let (lo, hi) = (lum.iter().min().unwrap(), lum.iter().max().unwrap());
            assert!(hi > lo, "{name} isn't flat");
        }
        let lin = MapGen { source: 2, p0: [0.0, 0.5], p1: [1.0, 0.5], ..MapGen::default() };
        assert!((lin.value(0.25, 0.3) - 0.25).abs() < 1e-5, "linear runs from p0 to p1");
    }
}
