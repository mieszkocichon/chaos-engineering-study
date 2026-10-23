//! Port of `seedrandom@3.0.5` (ARC4, RC4-drop[256]) + the distributions of
//! `when-gossip-goes-silent/lib/prng.js`. Same seed string ⇒ same sequence as Node (FR-005).

use metastable::jsmath;

const WIDTH: f64 = 256.0;
const MASK: usize = 255;
const CHUNKS: u32 = 6;
const STARTDENOM: f64 = 281_474_976_710_656.0; // 256^6
const SIGNIFICANCE: f64 = 4_503_599_627_370_496.0; // 2^52
const OVERFLOW: f64 = SIGNIFICANCE * 2.0;

struct Arc4 {
    i: usize,
    j: usize,
    s: [usize; 256],
}

impl Arc4 {
    fn new(key: &[usize]) -> Self {
        let key: &[usize] = if key.is_empty() { &[0] } else { key };
        let mut s = [0usize; 256];
        for (i, v) in s.iter_mut().enumerate() {
            *v = i;
        }
        let mut j = 0;
        for i in 0..256 {
            let t = s[i];
            j = MASK & (j + key[i % key.len()] + t);
            s[i] = s[j];
            s[j] = t;
        }
        let mut a = Arc4 { i: 0, j: 0, s };
        a.g(256); // RC4-drop[256]
        a
    }

    fn g(&mut self, count: u32) -> f64 {
        let (mut i, mut j) = (self.i, self.j);
        let s = &mut self.s;
        let mut r = 0.0;
        for _ in 0..count {
            i = MASK & (i + 1);
            let t = s[i];
            j = MASK & (j + t);
            s[i] = s[j];
            s[j] = t;
            r = r * WIDTH + s[MASK & (s[i] + s[j])] as f64;
        }
        self.i = i;
        self.j = j;
        r
    }
}

/// `mixkey(seed, [])`: string seed → ARC4 key (UTF-16 code units, like `charCodeAt`).
fn mixkey(seed: &str) -> Vec<usize> {
    let mut key: Vec<Option<usize>> = Vec::new();
    let mut smear: i64 = 0;
    for (j, code) in seed.encode_utf16().enumerate() {
        let idx = MASK & j;
        if key.len() <= idx {
            key.resize(idx + 1, None);
        }
        // `undefined * 19` is NaN, and `smear ^= NaN` xors with 0
        smear ^= key[idx].map_or(0, |k| (k * 19) as i64);
        key[idx] = Some(MASK & (smear + code as i64) as usize);
    }
    key.into_iter().map(|k| k.unwrap_or(0)).collect()
}

pub struct Prng {
    arc4: Arc4,
}

impl Prng {
    /// `createPRNG(seed)` – the seed is used as a string (`String(seed)`).
    pub fn new(seed: &str) -> Self {
        Prng { arc4: Arc4::new(&mixkey(seed)) }
    }

    /// Double in [0, 1) with 52 random mantissa bits.
    pub fn random(&mut self) -> f64 {
        let mut n = self.arc4.g(CHUNKS);
        let mut d = STARTDENOM;
        let mut x = 0.0f64;
        while n < SIGNIFICANCE {
            n = (n + x) * WIDTH;
            d *= WIDTH;
            x = self.arc4.g(1);
        }
        while n >= OVERFLOW {
            n /= 2.0;
            d /= 2.0;
            x = ((x as u32) >> 1) as f64;
        }
        (n + x) / d
    }

    pub fn bernoulli(&mut self, p: f64) -> bool {
        self.random() < p
    }

    /// Inclusive on both ends, like `min + floor(random() * (max - min + 1))`.
    pub fn uniform(&mut self, min: f64, max: f64) -> f64 {
        min + (self.random() * (max - min + 1.0)).floor()
    }

    /// Box–Muller, with V8-identical log/cos.
    pub fn normal_random(&mut self, mean: f64, std: f64) -> f64 {
        let u1 = self.random();
        let u2 = self.random();
        let z = (-2.0 * jsmath::log(u1)).sqrt() * jsmath::cos(2.0 * std::f64::consts::PI * u2);
        mean + z * std
    }

    pub fn exponential(&mut self, lambda: f64) -> f64 {
        -jsmath::log(1.0 - self.random()) / lambda
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixkey_short_and_wrapping_seeds() {
        assert_eq!(mixkey(""), Vec::<usize>::new());
        assert_eq!(mixkey("A"), vec![65]);
        assert_eq!(mixkey(&"a".repeat(300)).len(), 256);
    }
}
