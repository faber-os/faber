//! A small seeded random number generator for sensor noise. The simulation must be exactly
//! repeatable from a seed, so tests can compare readings; this is SplitMix64, which is enough
//! for noise and needs no dependency.

#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Normally distributed with mean 0 and standard deviation `sigma` (Box–Muller).
    pub fn gaussian(&mut self, sigma: f64) -> f64 {
        if sigma == 0.0 {
            return 0.0;
        }
        let u1 = 1.0 - self.uniform(); // (0, 1]: ln stays finite
        let u2 = self.uniform();
        sigma * (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Rng::new(7).next_u64(), Rng::new(8).next_u64());
    }

    #[test]
    fn gaussian_has_the_asked_spread() {
        let mut r = Rng::new(1);
        let n = 20_000;
        let xs: Vec<f64> = (0..n).map(|_| r.gaussian(0.01)).collect();
        let mean = xs.iter().sum::<f64>() / n as f64;
        let sd = (xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64).sqrt();
        assert!(mean.abs() < 0.0005, "mean {mean}");
        assert!((sd - 0.01).abs() < 0.0005, "sd {sd}");
        assert!((0..1000).all(|_| (0.0..1.0).contains(&r.uniform())));
        assert_eq!(r.gaussian(0.0), 0.0);
    }
}
