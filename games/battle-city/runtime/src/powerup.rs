//! The six power-ups from 策划案 appendix P.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PowerKind {
    Grenade,
    Helmet,
    Shovel,
    Star,
    Tank,
    Clock,
}

pub const ALL_POWERS: [PowerKind; 6] = [
    PowerKind::Grenade,
    PowerKind::Helmet,
    PowerKind::Shovel,
    PowerKind::Star,
    PowerKind::Tank,
    PowerKind::Clock,
];

impl PowerKind {
    pub fn index(self) -> usize {
        match self {
            PowerKind::Grenade => 0,
            PowerKind::Helmet => 1,
            PowerKind::Shovel => 2,
            PowerKind::Star => 3,
            PowerKind::Tank => 4,
            PowerKind::Clock => 5,
        }
    }

    pub fn from_index(index: u32) -> Self {
        ALL_POWERS[(index as usize) % ALL_POWERS.len()]
    }

    pub fn asset(self) -> &'static str {
        match self {
            PowerKind::Grenade => "power_grenade",
            PowerKind::Helmet => "power_helmet",
            PowerKind::Shovel => "power_shovel",
            PowerKind::Star => "power_star",
            PowerKind::Tank => "power_tank",
            PowerKind::Clock => "power_clock",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PowerKind::Grenade => "手雷",
            PowerKind::Helmet => "头盔",
            PowerKind::Shovel => "铲子",
            PowerKind::Star => "星星",
            PowerKind::Tank => "加命",
            PowerKind::Clock => "时钟",
        }
    }

    pub fn score(self) -> u32 {
        500
    }
}

/// Timed effects, all in seconds of simulation time.
pub const SHIELD_SECS: f32 = 3.0;
pub const HELMET_SECS: f32 = 10.0;
pub const SHOVEL_SECS: f32 = 20.0;
pub const CLOCK_SECS: f32 = 10.0;
pub const POWERUP_LIFE: f32 = 16.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_power_has_a_delivered_asset() {
        for power in ALL_POWERS {
            assert!(power.asset().starts_with("power_"));
            assert_eq!(power.score(), 500);
        }
    }
}
