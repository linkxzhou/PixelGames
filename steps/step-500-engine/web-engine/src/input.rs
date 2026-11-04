use miniquad::{KeyCode, KeyMods, MouseButton, TouchPhase};

pub const INPUT_CAPACITY: usize = 256;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InputEvent {
    pub kind: u32,
    pub code: u32,
    pub x: f32,
    pub y: f32,
}

pub struct InputQueue {
    events: [InputEvent; INPUT_CAPACITY],
    len: usize,
}

impl Default for InputQueue {
    fn default() -> Self {
        Self {
            events: [InputEvent::default(); INPUT_CAPACITY],
            len: 0,
        }
    }
}

impl InputQueue {
    pub fn push(&mut self, kind: u32, code: u32, x: f32, y: f32) {
        if self.len == INPUT_CAPACITY {
            self.events.copy_within(1.., 0);
            self.len -= 1;
        }
        self.events[self.len] = InputEvent { kind, code, x, y };
        self.len += 1;
    }

    pub fn key_down(&mut self, key: KeyCode, _mods: KeyMods, repeat: bool) {
        if !repeat {
            self.push(1, key as u32, 0.0, 0.0);
        }
    }

    pub fn key_up(&mut self, key: KeyCode) {
        self.push(2, key as u32, 0.0, 0.0);
    }

    pub fn mouse_move(&mut self, x: f32, y: f32) {
        self.push(3, 0, x, y);
    }

    pub fn mouse_button(&mut self, button: MouseButton, x: f32, y: f32, down: bool) {
        self.push(if down { 4 } else { 5 }, button as u32, x, y);
    }

    pub fn wheel(&mut self, x: f32, y: f32) {
        self.push(6, 0, x, y);
    }

    pub fn touch(&mut self, phase: TouchPhase, id: u64, x: f32, y: f32) {
        let phase = match phase {
            TouchPhase::Started => 0,
            TouchPhase::Moved => 1,
            TouchPhase::Ended => 2,
            TouchPhase::Cancelled => 3,
        };
        self.push(7, phase, x, y);
        self.push(8, id as u32, x, y);
    }

    pub fn ptr(&mut self) -> *const InputEvent {
        self.events.as_ptr()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }
}
