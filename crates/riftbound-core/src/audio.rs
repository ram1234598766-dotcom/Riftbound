//! Deterministic vehicle audio parameters for platform audio backends.
//!
//! This module does not open an audio device. It produces smoothed engine and
//! tyre mix controls so native and browser backends can render the same vehicle
//! sound behaviour without coupling the simulation to a specific audio API.

use crate::VehicleSurface;

/// Smoothed mix controls for one vehicle's engine and tyre sound layers.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleAudioState {
    engine_frequency_hz: f32,
    engine_gain: f32,
    tyre_gain: f32,
    skid_gain: f32,
    surface_gain: f32,
    impact_gain: f32,
}

/// Read-only audio parameters suitable for an audio backend or debug HUD.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleAudioMix {
    /// Engine oscillator/sample playback frequency in hertz.
    pub engine_frequency_hz: f32,
    /// Engine layer gain in the inclusive range 0..=1.
    pub engine_gain: f32,
    /// Rolling tyre layer gain in the inclusive range 0..=1.
    pub tyre_gain: f32,
    /// Tyre slip/skid layer gain in the inclusive range 0..=1.
    pub skid_gain: f32,
    /// Terrain texture layer gain in the inclusive range 0..=1.
    pub surface_gain: f32,
    /// Suspension impact layer gain in the inclusive range 0..=1.
    pub impact_gain: f32,
}

impl VehicleAudioState {
    /// Updates smoothed audio controls from vehicle motion and terrain.
    /// Invalid inputs leave the previous state unchanged.
    pub fn update(
        &mut self,
        speed: f32,
        throttle: f32,
        slip_ratios: [f32; 4],
        surface: VehicleSurface,
        delta_seconds: f32,
    ) -> VehicleAudioMix {
        self.update_with_impact(speed, throttle, slip_ratios, surface, 0.0, delta_seconds)
    }

    /// Updates audio controls with a normalized suspension-impact signal.
    /// Invalid inputs leave the previous state unchanged.
    pub fn update_with_impact(
        &mut self,
        speed: f32,
        throttle: f32,
        slip_ratios: [f32; 4],
        surface: VehicleSurface,
        impact_intensity: f32,
        delta_seconds: f32,
    ) -> VehicleAudioMix {
        if !speed.is_finite()
            || !throttle.is_finite()
            || !delta_seconds.is_finite()
            || !impact_intensity.is_finite()
            || slip_ratios.iter().any(|value| !value.is_finite())
            || delta_seconds <= 0.0
        {
            return self.mix();
        }

        let speed_factor = (speed.abs() / 42.0).clamp(0.0, 1.0);
        let throttle = throttle.clamp(0.0, 1.0);
        let max_slip = slip_ratios
            .iter()
            .fold(0.0_f32, |peak, slip| peak.max(slip.abs()))
            .clamp(0.0, 1.0);
        let (surface_noise, tyre_factor) = match surface {
            VehicleSurface::Asphalt => (0.12, 0.75),
            VehicleSurface::Gravel => (0.72, 1.0),
            VehicleSurface::Mud => (0.42, 0.82),
            VehicleSurface::Sand => (0.88, 1.05),
            VehicleSurface::Snow => (0.58, 0.92),
            VehicleSurface::Rock => (0.28, 0.88),
        };
        let targets = [
            55.0 + speed_factor * 82.0 + throttle * 28.0,
            (0.14 + throttle * 0.52 + speed_factor * 0.16).clamp(0.0, 0.9),
            (speed_factor * tyre_factor * 0.48).clamp(0.0, 0.65),
            (max_slip * speed_factor * 0.78).clamp(0.0, 0.8),
            (surface_noise * speed_factor).clamp(0.0, 0.8),
            impact_intensity.clamp(0.0, 1.0) * 0.72,
        ];
        let blend = (1.0 - (-delta_seconds.clamp(0.0, 0.1) * 12.0).exp()).clamp(0.0, 1.0);
        self.engine_frequency_hz = approach(self.engine_frequency_hz, targets[0], blend);
        self.engine_gain = approach(self.engine_gain, targets[1], blend);
        self.tyre_gain = approach(self.tyre_gain, targets[2], blend);
        self.skid_gain = approach(self.skid_gain, targets[3], blend);
        self.surface_gain = approach(self.surface_gain, targets[4], blend);
        let impact_blend = (1.0 - (-delta_seconds.clamp(0.0, 0.1) * 24.0).exp()).clamp(0.0, 1.0);
        self.impact_gain = approach(self.impact_gain, targets[5], impact_blend);
        self.mix()
    }

    /// Returns the current smoothed audio mix.
    #[must_use]
    pub const fn mix(self) -> VehicleAudioMix {
        VehicleAudioMix {
            engine_frequency_hz: self.engine_frequency_hz,
            engine_gain: self.engine_gain,
            tyre_gain: self.tyre_gain,
            skid_gain: self.skid_gain,
            surface_gain: self.surface_gain,
            impact_gain: self.impact_gain,
        }
    }
}

fn approach(current: f32, target: f32, blend: f32) -> f32 {
    current + (target - current) * blend
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicle_audio_tracks_throttle_speed_and_slip() {
        let mut audio = VehicleAudioState::default();
        let quiet = audio.update(0.0, 0.0, [0.0; 4], VehicleSurface::Asphalt, 0.1);
        let moving = audio.update(30.0, 0.9, [0.7, 0.1, 0.2, 0.1], VehicleSurface::Gravel, 0.1);
        assert!(moving.engine_frequency_hz > quiet.engine_frequency_hz);
        assert!(moving.engine_gain > quiet.engine_gain);
        assert!(moving.tyre_gain > quiet.tyre_gain);
        assert!(moving.skid_gain > quiet.skid_gain);
        assert!(moving.surface_gain > quiet.surface_gain);
    }

    #[test]
    fn suspension_impact_adds_a_smoothed_audio_layer() {
        let mut audio = VehicleAudioState::default();
        let quiet =
            audio.update_with_impact(8.0, 0.2, [0.0; 4], VehicleSurface::Gravel, 0.0, 0.016);
        let bump = audio.update_with_impact(8.0, 0.2, [0.0; 4], VehicleSurface::Gravel, 1.0, 0.016);
        assert!(bump.impact_gain > quiet.impact_gain);
        assert!((0.0..=1.0).contains(&bump.impact_gain));
        let settling =
            audio.update_with_impact(8.0, 0.2, [0.0; 4], VehicleSurface::Gravel, 0.0, 0.016);
        assert!(settling.impact_gain < bump.impact_gain);
    }

    #[test]
    fn invalid_audio_input_does_not_mutate_state() {
        let mut audio = VehicleAudioState::default();
        let before = audio.mix();
        let after = audio.update(f32::NAN, 1.0, [0.0; 4], VehicleSurface::Mud, 0.016);
        assert_eq!(before, after);
    }

    #[test]
    fn audio_gains_remain_bounded() {
        let mut audio = VehicleAudioState::default();
        let mix = audio.update(10_000.0, 10.0, [10.0; 4], VehicleSurface::Sand, 0.1);
        for gain in [
            mix.engine_gain,
            mix.tyre_gain,
            mix.skid_gain,
            mix.surface_gain,
        ] {
            assert!((0.0..=1.0).contains(&gain));
        }
    }
}
