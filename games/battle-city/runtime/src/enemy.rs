//! Enemy classes: the only place their numbers live.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnemyKind {
    Basic,
    Fast,
    Power,
    Armor,
}

pub const ALL_KINDS: [EnemyKind; 4] = [
    EnemyKind::Basic,
    EnemyKind::Fast,
    EnemyKind::Power,
    EnemyKind::Armor,
];

impl EnemyKind {
    pub fn index(self) -> usize {
        match self {
            EnemyKind::Basic => 0,
            EnemyKind::Fast => 1,
            EnemyKind::Power => 2,
            EnemyKind::Armor => 3,
        }
    }

    pub fn from_index(index: usize) -> Self {
        ALL_KINDS[index.min(3)]
    }

    /// Asset id authored by the pxl pipeline.
    pub fn asset(self) -> &'static str {
        match self {
            EnemyKind::Basic => "enemy_basic",
            EnemyKind::Fast => "enemy_fast",
            EnemyKind::Power => "enemy_power",
            EnemyKind::Armor => "enemy_armor",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            EnemyKind::Basic => "基础",
            EnemyKind::Fast => "快速",
            EnemyKind::Power => "强力",
            EnemyKind::Armor => "装甲",
        }
    }

    /// Pixels per second.
    pub fn speed(self) -> f32 {
        match self {
            EnemyKind::Basic => 46.0,
            EnemyKind::Fast => 88.0,
            EnemyKind::Power => 54.0,
            EnemyKind::Armor => 48.0,
        }
    }

    /// Seconds between shells.
    pub fn fire_cd(self) -> f32 {
        match self {
            EnemyKind::Basic => 2.6,
            EnemyKind::Fast => 2.2,
            EnemyKind::Power => 1.35,
            EnemyKind::Armor => 2.0,
        }
    }

    pub fn shell_speed(self) -> f32 {
        match self {
            EnemyKind::Power => 330.0,
            _ => 260.0,
        }
    }

    pub fn hp(self) -> u8 {
        match self {
            EnemyKind::Armor => 4,
            _ => 1,
        }
    }

    pub fn score(self) -> u32 {
        match self {
            EnemyKind::Basic => 100,
            EnemyKind::Fast => 200,
            EnemyKind::Power => 300,
            EnemyKind::Armor => 400,
        }
    }

    /// How strongly this class drives at the HQ instead of wandering.
    pub fn base_bias(self) -> f32 {
        match self {
            EnemyKind::Fast => 0.92,
            EnemyKind::Basic => 0.55,
            EnemyKind::Power => 0.6,
            EnemyKind::Armor => 0.45,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armour_is_the_only_multi_hit_class() {
        assert_eq!(EnemyKind::Armor.hp(), 4);
        assert_eq!(EnemyKind::Basic.hp(), 1);
        assert!(EnemyKind::Fast.speed() > EnemyKind::Basic.speed());
        assert_eq!(EnemyKind::Power.score(), 300);
    }
}
