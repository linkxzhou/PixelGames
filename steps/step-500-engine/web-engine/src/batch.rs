pub const COMMAND_WORDS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Clear(u32),
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rgba: u32,
    },
    Sprite {
        texture: u32,
        src: [f32; 4],
        dst: [f32; 4],
        flip_x: bool,
        flip_y: bool,
        tint: u32,
    },
    Camera(f32, f32),
    Light {
        x: f32,
        y: f32,
        radius: f32,
        rgba: u32,
        levels: u32,
    },
}

pub fn decode(words: &[u32]) -> Option<Command> {
    if words.len() < COMMAND_WORDS {
        return None;
    }
    let f = |index: usize| f32::from_bits(words[index]);
    match words[0] {
        1 => Some(Command::Clear(words[1])),
        2 => Some(Command::Rect {
            x: f(1),
            y: f(2),
            w: f(3),
            h: f(4),
            rgba: words[5],
        }),
        3 => Some(Command::Sprite {
            texture: words[1],
            src: [f(2), f(3), f(4), f(5)],
            dst: [f(6), f(7), f(8), f(9)],
            flip_x: words[10] & 1 != 0,
            flip_y: words[10] & 2 != 0,
            tint: words[11],
        }),
        4 => Some(Command::Camera(f(1), f(2))),
        5 => Some(Command::Light {
            x: f(1),
            y: f(2),
            radius: f(3),
            rgba: words[4],
            levels: words[5],
        }),
        _ => None,
    }
}

pub fn color(rgba: u32) -> [f32; 4] {
    [
        ((rgba >> 24) & 0xff) as f32 / 255.0,
        ((rgba >> 16) & 0xff) as f32 / 255.0,
        ((rgba >> 8) & 0xff) as f32 / 255.0,
        (rgba & 0xff) as f32 / 255.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_sprite_and_color() {
        let mut words = [0u32; COMMAND_WORDS];
        words[0] = 3;
        words[1] = 7;
        words[2] = 1.0f32.to_bits();
        words[6] = 10.0f32.to_bits();
        words[10] = 3;
        words[11] = 0x804020ff;
        assert_eq!(
            decode(&words),
            Some(Command::Sprite {
                texture: 7,
                src: [1.0, 0.0, 0.0, 0.0],
                dst: [10.0, 0.0, 0.0, 0.0],
                flip_x: true,
                flip_y: true,
                tint: 0x804020ff,
            })
        );
        assert_eq!(color(0xff000080), [1.0, 0.0, 0.0, 128.0 / 255.0]);
    }

    #[test]
    fn decodes_quantized_light() {
        let mut words = [0u32; COMMAND_WORDS];
        words[0] = 5;
        words[1] = 12.0f32.to_bits();
        words[2] = 8.0f32.to_bits();
        words[3] = 24.0f32.to_bits();
        words[4] = 0xff8040ff;
        words[5] = 4;
        assert_eq!(
            decode(&words),
            Some(Command::Light {
                x: 12.0,
                y: 8.0,
                radius: 24.0,
                rgba: 0xff8040ff,
                levels: 4,
            })
        );
    }
}
