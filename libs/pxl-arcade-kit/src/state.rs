//! The arcade flow every remake shares:
//! `Attract -> Ready -> Play -> StageClear -> Ready ... -> GameOver -> HighScore`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    /// Title demo: the simulation runs unattended behind `PRESS START`.
    Attract,
    /// Coin accepted, showing `READY!` for a moment.
    Ready,
    Play,
    Paused,
    StageClear,
    GameOver,
    HighScore,
}

impl ScreenState {
    pub fn is_play(self) -> bool {
        matches!(self, ScreenState::Play)
    }

    /// States where the timer still advances the simulation clock.
    pub fn runs_clock(self) -> bool {
        matches!(
            self,
            ScreenState::Attract | ScreenState::Ready | ScreenState::Play | ScreenState::StageClear
        )
    }

    pub fn accepts_input(self) -> bool {
        matches!(self, ScreenState::Play)
    }
}

/// Milliseconds a transition screen holds before it moves on.
pub const READY_MS: u32 = 1600;
pub const STAGE_CLEAR_MS: u32 = 2200;
pub const GAME_OVER_MS: u32 = 2600;
