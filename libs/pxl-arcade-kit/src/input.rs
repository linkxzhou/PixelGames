//! Platform-neutral input snapshot.
//!
//! `Buttons` is the replayable unit: the window layer fills it from the
//! keyboard and gamepad, a replay file fills it from the recording, and the
//! simulation only ever sees `InputState`.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Buttons {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub primary: bool,
    pub secondary: bool,
    pub start: bool,
    pub pause: bool,
    pub coin: bool,
}

impl Buttons {
    pub const NONE: Buttons = Buttons {
        up: false,
        down: false,
        left: false,
        right: false,
        primary: false,
        secondary: false,
        start: false,
        pause: false,
        coin: false,
    };

    pub fn any(&self) -> bool {
        *self != Buttons::NONE
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir4 {
    Up,
    Down,
    Left,
    Right,
}

impl Dir4 {
    pub fn delta(self) -> (i32, i32) {
        match self {
            Dir4::Up => (0, -1),
            Dir4::Down => (0, 1),
            Dir4::Left => (-1, 0),
            Dir4::Right => (1, 0),
        }
    }

    pub fn opposite(self) -> Dir4 {
        match self {
            Dir4::Up => Dir4::Down,
            Dir4::Down => Dir4::Up,
            Dir4::Left => Dir4::Right,
            Dir4::Right => Dir4::Left,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Dir4::Up => "N",
            Dir4::Down => "S",
            Dir4::Left => "W",
            Dir4::Right => "E",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputState {
    pub held: Buttons,
    pub pressed: Buttons,
}

impl InputState {
    pub fn new(held: Buttons, previous: Buttons) -> InputState {
        InputState {
            held,
            pressed: Buttons {
                up: held.up && !previous.up,
                down: held.down && !previous.down,
                left: held.left && !previous.left,
                right: held.right && !previous.right,
                primary: held.primary && !previous.primary,
                secondary: held.secondary && !previous.secondary,
                start: held.start && !previous.start,
                pause: held.pause && !previous.pause,
                coin: held.coin && !previous.coin,
            },
        }
    }

    pub fn idle() -> InputState {
        InputState::default()
    }

    /// Held direction, horizontal first (arcade sticks are wired that way).
    pub fn dir_held(&self) -> Option<Dir4> {
        if self.held.left {
            Some(Dir4::Left)
        } else if self.held.right {
            Some(Dir4::Right)
        } else if self.held.up {
            Some(Dir4::Up)
        } else if self.held.down {
            Some(Dir4::Down)
        } else {
            None
        }
    }

    /// Direction that went down this tick: use it for buffered turns and hops.
    pub fn dir_pressed(&self) -> Option<Dir4> {
        if self.pressed.left {
            Some(Dir4::Left)
        } else if self.pressed.right {
            Some(Dir4::Right)
        } else if self.pressed.up {
            Some(Dir4::Up)
        } else if self.pressed.down {
            Some(Dir4::Down)
        } else {
            None
        }
    }

    pub fn primary(&self) -> bool {
        self.held.primary
    }

    pub fn primary_pressed(&self) -> bool {
        self.pressed.primary
    }

    pub fn secondary_pressed(&self) -> bool {
        self.pressed.secondary
    }

    pub fn start_pressed(&self) -> bool {
        self.pressed.start
    }

    pub fn pause_pressed(&self) -> bool {
        self.pressed.pause
    }

    pub fn coin_pressed(&self) -> bool {
        self.pressed.coin || self.pressed.start
    }
}
