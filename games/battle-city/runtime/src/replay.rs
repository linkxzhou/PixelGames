//! Headless deterministic replay. Writes palette-checked PNG screenshots.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::draw::{self, Frame, LOGIC_H, LOGIC_W};
use crate::level::Level;
use crate::palette::Palette;
use crate::pngutil::{self, scale_nearest};
use crate::sim::{Dir, Input, Sim};

#[derive(Debug, Clone, Deserialize)]
pub struct Replay {
    pub level: u32,
    #[serde(default)]
    pub seed: u64,
    pub frames: u32,
    pub script: Vec<Cue>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Cue {
    pub until: u32,
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub shoot: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayReport {
    pub ok: bool,
    pub panicked: bool,
    pub frames: u32,
    pub level: u32,
    pub seed: u64,
    pub player_moved: bool,
    pub shots: u32,
    pub bricks_destroyed: u32,
    pub enemies_destroyed: u32,
    pub palette_mismatches: u32,
    pub player_start: [i32; 2],
    pub player_end: [i32; 2],
    pub outcome: String,
    pub state_hash: String,
    pub screenshots: Vec<String>,
    pub fixed_dt_hz: u32,
}

pub fn bundled_playtest() -> Replay {
    serde_json::from_str(include_str!("../replay/playtest.json")).expect("playtest replay")
}

pub struct HeadlessRequest {
    pub replay: Replay,
    pub out_dir: PathBuf,
    pub scale: u32,
}

pub fn run_replay(req: &HeadlessRequest) -> Result<PlayReport, String> {
    if req.replay.frames == 0 {
        return Err("replay.frames must be > 0".into());
    }
    let level = Level::bundled(req.replay.level)?;
    let mut sim = Sim::new(req.replay.level, level);
    let _ = req.replay.seed;
    let palette = Palette::battle_city();
    let mut frame = Frame::new();
    let mut mismatches = 0u32;
    let mut shots_written = Vec::new();
    let capture_at = capture_frames(req.replay.frames);
    std::fs::create_dir_all(&req.out_dir).map_err(|e| e.to_string())?;
    for tick in 0..req.replay.frames {
        let input = input_at(&req.replay, tick);
        sim.step(input);
        if capture_at.contains(&tick) || tick + 1 == req.replay.frames {
            draw::render(&sim, &mut frame);
            mismatches = mismatches.saturating_add(frame.mismatches(&palette));
            let name = format!("frame_{tick:04}.png");
            write_shot(&req.out_dir, &name, &frame, &palette, req.scale)?;
            shots_written.push(name);
        }
    }
    draw::render(&sim, &mut frame);
    mismatches = mismatches.saturating_add(frame.mismatches(&palette));
    let report = PlayReport {
        ok: mismatches == 0,
        panicked: false,
        frames: req.replay.frames,
        level: req.replay.level,
        seed: req.replay.seed,
        player_moved: sim.player_moved,
        shots: sim.shots,
        bricks_destroyed: sim.bricks_destroyed,
        enemies_destroyed: sim.enemies_destroyed,
        palette_mismatches: mismatches,
        player_start: [sim.player_start.0, sim.player_start.1],
        player_end: [sim.player.x, sim.player.y],
        outcome: format!("{:?}", sim.outcome).to_lowercase(),
        state_hash: sim.state_hash(),
        screenshots: shots_written,
        fixed_dt_hz: 60,
    };
    Ok(report)
}

fn capture_frames(frames: u32) -> Vec<u32> {
    let mut out = vec![0];
    if frames > 2 {
        out.push(frames / 2);
    }
    out
}

fn input_at(replay: &Replay, tick: u32) -> Input {
    for cue in &replay.script {
        if tick < cue.until {
            return Input {
                dir: cue.dir.as_deref().map(Dir::parse).filter(|_| cue.dir.as_deref() != Some("none")),
                shoot: cue.shoot,
            };
        }
    }
    Input::default()
}

fn write_shot(dir: &Path, name: &str, frame: &Frame, palette: &Palette, scale: u32) -> Result<(), String> {
    let rgba = frame.to_rgba(palette);
    let path = dir.join(name);
    pngutil::write_rgba(&path, LOGIC_W, LOGIC_H, &rgba)?;
    if scale > 1 {
        let big = scale_nearest(&rgba, LOGIC_W, LOGIC_H, scale);
        let scaled = dir.join(name.replace(".png", &format!(".x{scale}.png")));
        pngutil::write_rgba(&scaled, LOGIC_W * scale, LOGIC_H * scale, &big)?;
    }
    Ok(())
}

pub fn cli(args: Vec<String>) -> Result<(), String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "battle-city [--headless --replay FILE --out DIR --report FILE --scale N --frames N --level N]\n\
             window controls: arrows or WASD, space shoots, 1/2/3 select a stage"
        );
        return Ok(());
    }
    let headless = args.iter().any(|a| a == "--headless") || !cfg!(feature = "window");
    if !headless {
        return Err("window entry is in the binary, not the library".into());
    }
    let replay_path = flag(&args, "--replay");
    let mut replay = match replay_path {
        Some(path) => {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
            serde_json::from_str(&text).map_err(|e| e.to_string())?
        }
        None => bundled_playtest(),
    };
    if let Some(level) = flag(&args, "--level") {
        replay.level = level.parse().map_err(|_| format!("bad --level {level}"))?;
    }
    if let Some(frames) = flag(&args, "--frames") {
        replay.frames = frames.parse().map_err(|_| format!("bad --frames {frames}"))?;
    }
    let out_dir = flag(&args, "--out").unwrap_or_else(|| "playtest-out".into());
    let scale = flag(&args, "--scale")
        .unwrap_or_else(|| "3".into())
        .parse()
        .map_err(|_| "bad --scale".to_string())?;
    let report = run_replay(&HeadlessRequest {
        replay,
        out_dir: PathBuf::from(&out_dir),
        scale,
    })?;
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(path) = flag(&args, "--report") {
        if let Some(parent) = Path::new(&path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, &json).map_err(|e| format!("{path}: {e}"))?;
    }
    println!("{json}");
    if !report.ok {
        return Err(format!("palette mismatches: {}", report.palette_mismatches));
    }
    Ok(())
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pngutil::read_rgba;

    #[test]
    fn bundled_replay_writes_a_palette_clean_screenshot() {
        let dir = std::env::temp_dir().join(format!("bc_replay_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let report = run_replay(&HeadlessRequest {
            replay: bundled_playtest(),
            out_dir: dir.clone(),
            scale: 1,
        })
        .unwrap();
        assert!(report.player_moved);
        assert!(report.shots >= 1);
        assert!(report.bricks_destroyed >= 1);
        assert_eq!(report.palette_mismatches, 0);
        assert!(!report.panicked);
        assert_eq!(report.fixed_dt_hz, 60);
        let shot = dir.join(&report.screenshots[0]);
        let (w, h, rgba) = read_rgba(&shot).unwrap();
        assert_eq!((w, h), (LOGIC_W, LOGIC_H));
        let palette = Palette::battle_city();
        let bad = rgba.chunks_exact(4).filter(|px| !palette.contains_rgba([px[0], px[1], px[2], px[3]])).count();
        assert_eq!(bad, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
