//! Lightweight deterministic enemy perception and encounter state machine.

use crate::Vec3;

/// Current state of a roaming arena sentinel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnemyState {
    /// Moving between its spawn point and patrol destination.
    Patrol,
    /// Pursuing a player inside its awareness radius.
    Chase,
    /// Brief, readable wind-up before a close-range pulse.
    Telegraph,
    /// Short recovery window after a pulse.
    Recover,
    /// Returning toward its home position after losing the player.
    Return,
}

/// Tunable distances and timings for a lightweight enemy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnemyConfig {
    /// Maximum distance at which the enemy notices the player.
    pub awareness_radius: f32,
    /// Distance at which the enemy begins its telegraph.
    pub attack_radius: f32,
    /// Distance at which the enemy abandons pursuit.
    pub leash_radius: f32,
    /// Movement speed while patrolling.
    pub patrol_speed: f32,
    /// Movement speed while pursuing or returning.
    pub chase_speed: f32,
    /// Seconds spent telegraphing an attack.
    pub telegraph_seconds: f32,
    /// Seconds spent recovering after an attack.
    pub recovery_seconds: f32,
}

impl Default for EnemyConfig {
    fn default() -> Self {
        Self {
            awareness_radius: 8.0,
            attack_radius: 1.7,
            leash_radius: 12.0,
            patrol_speed: 1.1,
            chase_speed: 2.4,
            telegraph_seconds: 0.65,
            recovery_seconds: 0.8,
        }
    }
}

/// Events emitted on state transitions; the runtime decides how to present them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnemyEvent {
    /// The player was detected.
    PlayerDetected,
    /// The enemy entered its telegraph window.
    AttackTelegraphed,
    /// The telegraph completed while the player remained in range.
    PulseReleased,
    /// The enemy lost the player and is returning home.
    ReturningHome,
}

/// Deterministic, allocation-free enemy brain for a single arena sentinel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnemyBrain {
    home: Vec3,
    state: EnemyState,
    timer: f32,
    config: EnemyConfig,
}

impl EnemyBrain {
    /// Creates an enemy at its home position.
    #[must_use]
    pub const fn new(home: Vec3, config: EnemyConfig) -> Self {
        Self {\n            home,\n            state: EnemyState::Patrol,\n            timer: 0.0,\n            config,\n        }
    }

    /// Current state.
    #[must_use]
    pub const fn state(self) -> EnemyState {\n        self.state\n    }

    /// Home position used to bound pursuit.
    #[must_use]
    pub const fn home(self) -> Vec3 {\n        self.home\n    }

    /// Advances the state machine and returns at most one transition event.
    ///
    /// Invalid positions, configuration values, and time steps leave state unchanged.
    pub fn update(\n        &mut self,\n        position: &mut Vec3,\n        player: Vec3,\n        delta_seconds: f32,\n    ) -> Option<EnemyEvent> {
        let c = self.config;
        if !valid_position(*position) || !valid_position(player) || !valid_position(self.home)
            || !delta_seconds.is_finite() || delta_seconds <= 0.0
            || !c.awareness_radius.is_finite() || c.awareness_radius <= 0.0
            || !c.attack_radius.is_finite() || c.attack_radius <= 0.0
            || !c.leash_radius.is_finite() || c.leash_radius < c.awareness_radius
            || !c.patrol_speed.is_finite() || c.patrol_speed < 0.0
            || !c.chase_speed.is_finite() || c.chase_speed < 0.0
            || !c.telegraph_seconds.is_finite() || c.telegraph_seconds <= 0.0
            || !c.recovery_seconds.is_finite() || c.recovery_seconds <= 0.0 {
            return None;
        }

        let delta = delta_seconds.min(0.1);
        let player_distance = distance_xz(*position, player);
        let home_distance = distance_xz(*position, self.home);

        match self.state {
            EnemyState::Patrol => {
                if player_distance <= c.awareness_radius {
                    self.state = EnemyState::Chase;
                    Some(EnemyEvent::PlayerDetected)
                } else {
                    None
                }
            }
            EnemyState::Chase => {
                if home_distance > c.leash_radius || player_distance > c.leash_radius {
                    self.state = EnemyState::Return;
                    Some(EnemyEvent::ReturningHome)
                } else if player_distance <= c.attack_radius {
                    self.state = EnemyState::Telegraph;
                    self.timer = 0.0;
                    Some(EnemyEvent::AttackTelegraphed)
                } else {
                    move_toward(position, player, c.chase_speed * delta);
                    None
                }
            }
            EnemyState::Telegraph => {
                if player_distance > c.attack_radius * 1.35 {
                    self.state = EnemyState::Chase;
                    self.timer = 0.0;
                    None
                } else {
                    self.timer += delta;
                    if self.timer >= c.telegraph_seconds {
                        self.state = EnemyState::Recover;
                        self.timer = 0.0;
                        Some(EnemyEvent::PulseReleased)
                    } else {
                        None
                    }
                }
            }
            EnemyState::Recover => {
                self.timer += delta;
                if self.timer >= c.recovery_seconds {
                    self.timer = 0.0;
                    self.state = if player_distance <= c.leash_radius {
                        EnemyState::Chase
                    } else {
                        EnemyState::Return
                    };
                }
                None
            }
            EnemyState::Return => {
                if home_distance <= 0.08 {
                    self.state = EnemyState::Patrol;
                    None
                } else {
                    move_toward(position, self.home, c.patrol_speed * delta);
                    None
                }
            }
        }
    }
}

fn valid_position(p: Vec3) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.z.is_finite()
}

fn distance_xz(a: Vec3, b: Vec3) -> f32 {
    let x = a.x - b.x;
    let z = a.z - b.z;
    x.mul_add(x, z * z).sqrt()
}

fn move_toward(position: &mut Vec3, target: Vec3, max_step: f32) {
    let dx = target.x - position.x;
    let dz = target.z - position.z;
    let distance = dx.mul_add(dx, dz * dz).sqrt();
    if distance <= max_step || distance <= f32::EPSILON {
        position.x = target.x;
        position.z = target.z;
    } else {
        let scale = max_step / distance;
        position.x += dx * scale;
        position.z += dz * scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brain() -> EnemyBrain {
        EnemyBrain::new(Vec3::new(0.0, 0.0, 0.0), EnemyConfig::default())
    }

    #[test]
    fn detects_player_and_moves_during_chase() {
        let mut brain = brain();
        let mut position = Vec3::new(0.0, 0.0, 0.0);
        let player = Vec3::new(5.0, 0.0, 0.0);
        assert_eq!(\n            brain.update(&mut position, player, 1.0 / 60.0),\n            Some(EnemyEvent::PlayerDetected)\n        );
        assert_eq!(brain.state(), EnemyState::Chase);
        brain.update(&mut position, player, 0.1);
        assert!(position.x > 0.0 && position.x < player.x);
    }

    #[test]
    fn telegraph_is_cancelled_when_player_moves_away() {
        let mut brain = brain();
        let mut position = Vec3::new(0.0, 0.0, 0.0);
        let near = Vec3::new(1.0, 0.0, 0.0);
        brain.update(&mut position, near, 0.01);
        assert_eq!(\n            brain.update(&mut position, near, 0.01),\n            Some(EnemyEvent::AttackTelegraphed)\n        );
        brain.update(&mut position, Vec3::new(5.0, 0.0, 0.0), 0.05);
        assert_eq!(brain.state(), EnemyState::Chase);
    }

    #[test]
    fn telegraph_releases_one_pulse_then_recovers() {
        let mut brain = brain();
        let mut position = Vec3::new(0.0, 0.0, 0.0);
        let near = Vec3::new(1.0, 0.0, 0.0);
        brain.update(&mut position, near, 0.01);
        brain.update(&mut position, near, 0.01);
        let mut released = false;
        for _ in 0..10 {
            released |= brain.update(&mut position, near, 0.1) == Some(EnemyEvent::PulseReleased);
        }
        assert!(released);
        assert_eq!(brain.state(), EnemyState::Recover);
    }

    #[test]
    fn invalid_delta_does_not_change_state_or_position() {
        let mut brain = brain();
        let mut position = Vec3::new(0.0, 0.0, 0.0);
        assert_eq!(\n            brain.update(&mut position, Vec3::new(2.0, 0.0, 0.0), f32::NAN),\n            None\n        );
        assert_eq!(brain.state(), EnemyState::Patrol);
        assert_eq!(position, Vec3::new(0.0, 0.0, 0.0));
    }
}
