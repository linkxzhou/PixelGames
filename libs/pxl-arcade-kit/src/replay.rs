//! Deterministic input tape and a stable state hash for regression tests.

use crate::input::Buttons;

#[derive(Debug, Clone, Default)]
pub struct Tape {
    pub seed: u64,
    pub inputs: Vec<Buttons>,
}

impl Tape {
    pub fn new(seed: u64) -> Tape {
        Tape {
            seed,
            inputs: Vec::new(),
        }
    }

    pub fn record(&mut self, buttons: Buttons) {
        self.inputs.push(buttons);
    }

    pub fn get(&self, tick: usize) -> Buttons {
        self.inputs.get(tick).copied().unwrap_or(Buttons::NONE)
    }

    pub fn len(&self) -> usize {
        self.inputs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    /// Fold the tape itself into a hash so a divergent recording is visible.
    pub fn digest(&self) -> u64 {
        let mut hasher = StateHash::new(self.seed);
        for buttons in &self.inputs {
            hasher.mix_u64(bits(*buttons));
        }
        hasher.finish()
    }
}

fn bits(buttons: Buttons) -> u64 {
    let mut value = 0u64;
    for (index, flag) in [
        buttons.up,
        buttons.down,
        buttons.left,
        buttons.right,
        buttons.primary,
        buttons.secondary,
        buttons.start,
        buttons.pause,
        buttons.coin,
    ]
    .iter()
    .enumerate()
    {
        if *flag {
            value |= 1 << index;
        }
    }
    value
}

/// FNV-style rolling hash with a SplitMix finaliser: order sensitive, cheap
/// and identical on every platform.
#[derive(Debug, Clone)]
pub struct StateHash {
    value: u64,
}

impl StateHash {
    pub fn new(seed: u64) -> StateHash {
        StateHash {
            value: 0xcbf2_9ce4_8422_2325 ^ seed,
        }
    }

    pub fn mix_u64(&mut self, value: u64) {
        self.value ^= value;
        self.value = self.value.wrapping_mul(0x0000_0100_0000_01b3);
        self.value ^= self.value >> 29;
    }

    pub fn mix_i64(&mut self, value: i64) {
        self.mix_u64(value as u64);
    }

    pub fn mix_str(&mut self, text: &str) {
        for byte in text.bytes() {
            self.mix_u64(byte as u64);
        }
        self.mix_u64(0xff);
    }

    pub fn finish(&self) -> u64 {
        let mut z = self.value;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}
