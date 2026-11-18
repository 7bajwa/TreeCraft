use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    pub const UP: V3 = v3(0.0, 1.0, 0.0);
    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: V3) -> V3 {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn norm(self) -> V3 {
        let l = self.len();
        if l > 1e-8 {
            self * (1.0 / l)
        } else {
            V3::UP
        }
    }
    /// Any unit vector perpendicular to self.
    pub fn perp(self) -> V3 {
        let a = if self.y.abs() < 0.9 { V3::UP } else { v3(1.0, 0.0, 0.0) };
        self.cross(a).norm()
    }
    /// Rotate around a unit axis by `ang` radians (Rodrigues).
    pub fn rotate(self, axis: V3, ang: f32) -> V3 {
        let (s, c) = ang.sin_cos();
        self * c + axis.cross(self) * s + axis * (axis.dot(self) * (1.0 - c))
    }
    pub fn min(self, o: V3) -> V3 {
        v3(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }
    pub fn max(self, o: V3) -> V3 {
        v3(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl AddAssign for V3 {
    fn add_assign(&mut self, o: V3) {
        *self = *self + o;
    }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, s: f32) -> V3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}

/// Small deterministic generator (SplitMix64) so a seed always grows the same tree.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in [0, 1).
    pub fn f(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    /// Uniform in [-1, 1).
    pub fn s(&mut self) -> f32 {
        self.f() * 2.0 - 1.0
    }
}

pub fn hash(a: u64, b: u64) -> u64 {
    Rng::new(a.wrapping_mul(0x100_0000_01B3) ^ b).next_u64()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotate_keeps_length_and_quarter_turn() {
        let r = v3(1.0, 0.0, 0.0).rotate(V3::UP, std::f32::consts::FRAC_PI_2);
        assert!((r.len() - 1.0).abs() < 1e-5);
        assert!((r.z + 1.0).abs() < 1e-5);
    }
    #[test]
    fn rng_is_repeatable_and_in_range() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..1000 {
            let x = a.f();
            assert!((0.0..1.0).contains(&x));
            assert_eq!(x, b.f());
        }
    }
}
