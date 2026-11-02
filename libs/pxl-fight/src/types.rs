//! A compact, deterministic 2D versus fighter.
//!
//! Two fighters stand on one arena floor. Attacks are frame-windowed (startup,
//! active, recovery), blocking chips damage, throws ignore block and a motion
//! command fires a projectile. Rounds are best of three and a ladder of
//! opponents advances the match.

use pxl_arcade_kit::input::{Dir4, InputState};
use pxl_arcade_kit::replay::StateHash;
use pxl_arcade_kit::rng::Rng;


pub const GROUND_Y: f32 = 176.0;
pub const STAGE_LEFT: f32 = 24.0;
pub const STAGE_RIGHT: f32 = 296.0;
pub const ROUND_MS: u32 = 60_000;
pub const INTRO_MS: u32 = 1400;
pub const ROUND_END_MS: u32 = 1800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    Punch,
    Kick,
    Special,
    Throw,
}

#[derive(Debug, Clone, Copy)]
pub struct MoveDef {
    pub kind: MoveKind,
    pub startup_ms: u32,
    pub active_ms: u32,
    pub recovery_ms: u32,
    pub damage: u32,
    pub reach: f32,
    pub ignores_block: bool,
    pub projectile: bool,
}

pub const PUNCH: MoveDef = MoveDef {
    kind: MoveKind::Punch,
    startup_ms: 60,
    active_ms: 70,
    recovery_ms: 110,
    damage: 6,
    reach: 36.0,
    ignores_block: false,
    projectile: false,
};

pub const KICK: MoveDef = MoveDef {
    kind: MoveKind::Kick,
    startup_ms: 110,
    active_ms: 90,
    recovery_ms: 170,
    damage: 9,
    reach: 48.0,
    ignores_block: false,
    projectile: false,
};

pub const SPECIAL: MoveDef = MoveDef {
    kind: MoveKind::Special,
    startup_ms: 160,
    active_ms: 20,
    recovery_ms: 240,
    damage: 14,
    reach: 0.0,
    ignores_block: false,
    projectile: true,
};

pub const THROW: MoveDef = MoveDef {
    kind: MoveKind::Throw,
    startup_ms: 40,
    active_ms: 60,
    recovery_ms: 220,
    damage: 12,
    reach: 22.0,
    ignores_block: true,
    projectile: false,
};

pub fn move_def(kind: MoveKind) -> MoveDef {
    match kind {
        MoveKind::Punch => PUNCH,
        MoveKind::Kick => KICK,
        MoveKind::Special => SPECIAL,
        MoveKind::Throw => THROW,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FighterDef {
    pub name: &'static str,
    pub tint: u8,
    pub walk_speed: f32,
    pub health: u32,
    pub ai_level: u8,
    pub special_damage: u32,
}

#[derive(Debug, Clone)]
pub struct Cast {
    pub player: FighterDef,
    pub opponents: Vec<FighterDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pose {
    Idle,
    Walk,
    Crouch,
    Jump,
    Punch,
    Kick,
    Block,
    Hit,
    Down,
    Special,
}

#[derive(Debug, Clone, Copy)]
pub struct Fighter {
    pub x: f32,
    pub y: f32,
    pub vy: f32,
    pub facing: i32,
    pub health: u32,
    pub pose: Pose,
    pub pose_ms: u32,
    pub walk_ms: u32,
    pub crouch: bool,
    pub on_ground: bool,
    pub attack: Option<(MoveKind, u32)>,
    pub hit_done: bool,
    pub stun_ms: u32,
    pub down: bool,
    pub flash_ms: u32,
}

impl Fighter {
    pub(crate) fn new(x: f32, facing: i32, health: u32) -> Fighter {
        Fighter {
            x,
            y: GROUND_Y,
            vy: 0.0,
            facing,
            health,
            pose: Pose::Idle,
            pose_ms: 0,
            walk_ms: 0,
            crouch: false,
            on_ground: true,
            attack: None,
            hit_done: false,
            stun_ms: 0,
            down: false,
            flash_ms: 0,
        }
    }

    pub fn anim(&self) -> &'static str {
        match self.pose {
            Pose::Idle => "idle",
            Pose::Walk => "walk",
            Pose::Crouch => "crouch",
            Pose::Jump => "walk",
            Pose::Punch => "punch",
            Pose::Kick => "kick",
            Pose::Block => "block",
            Pose::Hit => "hit",
            Pose::Down => "hit",
            Pose::Special => "punch",
        }
    }

    pub fn busy(&self) -> bool {
        self.attack.is_some() || self.stun_ms > 0 || self.down
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Projectile {
    pub x: f32,
    pub y: f32,
    pub dir: f32,
    pub damage: u32,
    pub owner: usize,
    pub alive: bool,
    pub age_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Intro(u32),
    Fight,
    RoundEnd(u32),
    /// Match finished; `true` means the player took it.
    MatchEnd(bool),
}

#[derive(Debug, Clone, Copy)]
pub struct Spark {
    pub x: f32,
    pub y: f32,
    pub age_ms: u32,
}

pub struct FightSim {
    pub player: Fighter,
    pub cpu: Fighter,
    pub player_def: FighterDef,
    pub cpu_def: FighterDef,
    pub projectiles: Vec<Projectile>,
    pub sparks: Vec<Spark>,
    pub phase: Phase,
    pub round: u8,
    pub wins: [u8; 2],
    pub timer_ms: u32,
    pub clock_ms: u64,
    pub ladder_index: usize,
    pub(crate) opponents: Vec<FighterDef>,
    pub(crate) rng: Rng,
    pub(crate) seed: u64,
    pub(crate) ai_timer_ms: u32,
    pub(crate) motion: Vec<(i32, u32)>,
    pub(crate) round_banner_ms: u32,
}

