//! A compact, deterministic 2D versus fighter.
//!
//! Two fighters stand on one arena floor. Attacks are frame-windowed (startup,
//! active, recovery), blocking chips damage, throws ignore block and a motion
//! command fires a projectile. Rounds are best of three and a ladder of
//! opponents advances the match.

use pxl_arcade_kit::input::{Dir4, InputState};
use pxl_arcade_kit::replay::StateHash;
use pxl_arcade_kit::rng::Rng;


use super::types::*;

impl FightSim {
    pub fn new(cast: &Cast, seed: u64) -> Result<FightSim, String> {
        if cast.opponents.is_empty() {
            return Err("the ladder is empty".into());
        }
        let mut sim = FightSim {
            player: Fighter::new(120.0, 1, cast.player.health),
            cpu: Fighter::new(200.0, -1, cast.opponents[0].health),
            player_def: cast.player,
            cpu_def: cast.opponents[0],
            projectiles: Vec::new(),
            sparks: Vec::new(),
            phase: Phase::Intro(INTRO_MS),
            round: 1,
            wins: [0, 0],
            timer_ms: ROUND_MS,
            clock_ms: 0,
            ladder_index: 0,
            opponents: cast.opponents.clone(),
            rng: Rng::new(seed),
            seed,
            ai_timer_ms: 0,
            motion: Vec::new(),
            round_banner_ms: 0,
        };
        sim.start_round();
        Ok(sim)
    }

    pub fn ladder_len(&self) -> usize {
        self.opponents.len()
    }

    pub fn opponent_name(&self) -> &'static str {
        self.cpu_def.name
    }

    pub(crate) fn start_round(&mut self) {
        let health = self.player_def.health;
        self.player = Fighter::new(110.0, 1, health);
        self.cpu = Fighter::new(210.0, -1, self.cpu_def.health);
        self.projectiles.clear();
        self.sparks.clear();
        self.timer_ms = ROUND_MS;
        self.phase = Phase::Intro(INTRO_MS);
        self.ai_timer_ms = 0;
        self.motion.clear();
        self.round_banner_ms = 0;
    }

    pub(crate) fn next_round(&mut self) {
        self.round = self.round.saturating_add(1);
        self.start_round();
    }

    pub(crate) fn next_opponent(&mut self) {
        self.ladder_index += 1;
        if self.ladder_index >= self.opponents.len() {
            self.phase = Phase::MatchEnd(true);
            return;
        }
        self.cpu_def = self.opponents[self.ladder_index];
        self.round = 1;
        self.wins = [0, 0];
        self.start_round();
    }

    pub fn restart(&mut self) {
        self.ladder_index = 0;
        self.cpu_def = self.opponents[0];
        self.round = 1;
        self.wins = [0, 0];
        self.rng = Rng::new(self.seed);
        self.start_round();
    }

    pub fn tick(&mut self, input: InputState, dt_ms: u32) {
        self.clock_ms += dt_ms as u64;
        match self.phase {
            Phase::Intro(timer) => {
                let left = timer.saturating_sub(dt_ms);
                if left == 0 {
                    self.phase = Phase::Fight;
                    self.round_banner_ms = 900;
                } else {
                    self.phase = Phase::Intro(left);
                }
                self.tick_poses(dt_ms);
                return;
            }
            Phase::RoundEnd(timer) => {
                let left = timer.saturating_sub(dt_ms);
                self.tick_poses(dt_ms);
                if left == 0 {
                    let player_won = self.wins[0] >= 2;
                    let cpu_won = self.wins[1] >= 2;
                    if cpu_won {
                        self.phase = Phase::MatchEnd(false);
                    } else if player_won {
                        self.next_opponent();
                    } else {
                        self.next_round();
                    }
                } else {
                    self.phase = Phase::RoundEnd(left);
                }
                return;
            }
            Phase::MatchEnd(_) => {
                if input.start_pressed() {
                    self.restart();
                }
                self.tick_poses(dt_ms);
                return;
            }
            Phase::Fight => {}
        }

        self.round_banner_ms = self.round_banner_ms.saturating_sub(dt_ms);
        self.timer_ms = self.timer_ms.saturating_sub(dt_ms);
        self.update_motion(input, dt_ms);
        self.update_player(input, dt_ms);
        self.update_cpu(dt_ms);
        self.update_projectiles(dt_ms);
        self.resolve_hits();
        self.update_sparks(dt_ms);

        if self.player.health == 0 || self.cpu.health == 0 || self.timer_ms == 0 {
            let player_won = if self.cpu.health == 0 {
                true
            } else if self.player.health == 0 {
                false
            } else {
                self.player.health >= self.cpu.health
            };
            if player_won {
                self.wins[0] += 1;
            } else {
                self.wins[1] += 1;
            }
            self.phase = Phase::RoundEnd(ROUND_END_MS);
        }
    }

    pub(crate) fn tick_poses(&mut self, dt_ms: u32) {
        self.player.pose_ms = self.player.pose_ms.saturating_add(dt_ms);
        self.cpu.pose_ms = self.cpu.pose_ms.saturating_add(dt_ms);
        for spark in &mut self.sparks {
            spark.age_ms += dt_ms;
        }
        self.sparks.retain(|spark| spark.age_ms < 240);
    }

    pub(crate) fn update_motion(&mut self, input: InputState, dt_ms: u32) {
        let now = self.clock_ms as u32;
        if let Some(dir) = input.dir_held() {
            let relative = match dir {
                Dir4::Left => -1,
                Dir4::Right => 1,
                _ => 0,
            };
            if relative != 0 {
                let last = self.motion.last().map(|entry| entry.0);
                if last != Some(relative) {
                    self.motion.push((relative, now));
                }
            }
        }
        self.motion.retain(|entry| now.saturating_sub(entry.1) < 600);
        let _ = dt_ms;
    }

    pub(crate) fn wants_special(&self) -> bool {
        let mut seen_away = false;
        for (dir, _) in &self.motion {
            let relative = dir * self.player.facing;
            if relative < 0 {
                seen_away = true;
            } else if relative > 0 && seen_away {
                return true;
            }
        }
        false
    }

    pub(crate) fn can_block(&self, fighter: &Fighter, opponent: &Fighter, input: InputState) -> bool {
        let away = match opponent.x > fighter.x {
            true => input.held.left,
            false => input.held.right,
        };
        away && fighter.on_ground && fighter.attack.is_none() && fighter.stun_ms == 0
    }

}
