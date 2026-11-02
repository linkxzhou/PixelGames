//! SplitMix64 mixes seeds. PCG32 streams integer draws.
//!
//! No platform `rand`, no floats. The same seed always yields the same stream.

use alloc::string::String;
use sha2::{Digest, Sha256};

/// SplitMix64, used only to derive subseeds.
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// PCG-XSH-RR 32-bit generator (Numerically stable, integer-only).
#[derive(Clone, Debug)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (stream << 1) | 1,
        };
        let _ = rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        let _ = rng.next_u32();
        rng
    }

    pub fn from_seed(seed: u64) -> Self {
        Self::new(seed, 0xDA3E_39CB_94B9_5BDB)
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Unbiased integer in `0..n`. `n == 0` yields 0.
    pub fn gen_range(&mut self, n: u32) -> u32 {
        if n <= 1 {
            return 0;
        }
        // Lemire's nearly-divisionless method, rejection included so the
        // distribution does not depend on floating point.
        let mut x = self.next_u32();
        let mut m = (x as u64).wrapping_mul(n as u64);
        let mut lo = m as u32;
        if lo < n {
            let threshold = n.wrapping_neg() % n;
            while lo < threshold {
                x = self.next_u32();
                m = (x as u64).wrapping_mul(n as u64);
                lo = m as u32;
            }
        }
        (m >> 32) as u32
    }

    pub fn chance_half(&mut self) -> bool {
        self.next_u32() & 1 == 1
    }

    /// True with probability `numer/denom` using integer rejection via [`gen_range`].
    pub fn chance(&mut self, numer: u32, denom: u32) -> bool {
        if denom == 0 || numer == 0 {
            return false;
        }
        if numer >= denom {
            return true;
        }
        self.gen_range(denom) < numer
    }

    pub fn next_u8(&mut self) -> u8 {
        (self.next_u32() & 0xff) as u8
    }
}

/// Subseed for one op: `hash(seed, op_index, frame_name)`.
pub fn op_seed(seed: u64, op_index: u64, frame_name: &str) -> u64 {
    let mut sm = SplitMix64::new(seed ^ op_index.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut h = sm.next_u64();
    for b in frame_name.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01B3);
    }
    h ^ sm.next_u64()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let dig = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for b in dig {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcg_is_stable() {
        let mut a = Pcg32::from_seed(42);
        let mut b = Pcg32::from_seed(42);
        let seq_a: alloc::vec::Vec<u32> = (0..8).map(|_| a.next_u32()).collect();
        let seq_b: alloc::vec::Vec<u32> = (0..8).map(|_| b.next_u32()).collect();
        assert_eq!(seq_a, seq_b);
        let mut c = Pcg32::from_seed(43);
        assert_ne!(seq_a[0], c.next_u32());
    }

    #[test]
    fn gen_range_stays_in_bounds() {
        let mut r = Pcg32::from_seed(7);
        for _ in 0..200 {
            let v = r.gen_range(3);
            assert!(v < 3);
        }
        assert_eq!(r.gen_range(0), 0);
        assert_eq!(r.gen_range(1), 0);
    }

    #[test]
    fn op_seed_depends_on_name_and_index() {
        assert_eq!(op_seed(1, 0, "base"), op_seed(1, 0, "base"));
        assert_ne!(op_seed(1, 0, "base"), op_seed(1, 1, "base"));
        assert_ne!(op_seed(1, 0, "base"), op_seed(1, 0, "N"));
    }
}
