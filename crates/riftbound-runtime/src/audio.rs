//! Lightweight synthesized vehicle audio output for the native runtime.
//!
//! The audio callback reads atomic mix controls and never touches the game
//! simulation or renderer. If no output device is available, the runtime can
//! continue without audio.

use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use std::time::Duration;

use rodio::{OutputStream, Sink, Source};
use riftbound_core::VehicleAudioMix;

const SAMPLE_RATE: u32 = 44_100;
const CONTROL_COUNT: usize = 6;

/// Owns the native output stream and a continuously generated vehicle sound.
pub struct VehicleAudioOutput {
    _stream: OutputStream,
    sink: Sink,
    controls: Arc<[AtomicU32; CONTROL_COUNT]>,
}

impl VehicleAudioOutput {
    /// Opens the default output device and starts a quiet vehicle sound source.
    pub fn new() -> Result<Self, String> {
        let (stream, stream_handle) =
            OutputStream::try_default().map_err(|error| format!("audio output unavailable: {error}"))?;
        let sink = Sink::try_new(&stream_handle)
            .map_err(|error| format!("audio sink unavailable: {error}"))?;
        let controls = Arc::new(std::array::from_fn(|_| AtomicU32::new(0.0_f32.to_bits())));
        sink.append(VehicleSource::new(Arc::clone(&controls)));
        sink.play();
        Ok(Self { _stream: stream, sink, controls })
    }

    /// Publishes the latest smoothed simulation mix without blocking playback.
    pub fn set_mix(&self, mix: VehicleAudioMix) {
        let values = [
            mix.engine_frequency_hz,
            mix.engine_gain,
            mix.tyre_gain,
            mix.skid_gain,
            mix.surface_gain,
            mix.impact_gain,
        ];
        for (control, value) in self.controls.iter().zip(values) {
            let safe = if value.is_finite() { value.clamp(0.0, 1_000.0) } else { 0.0 };
            control.store(safe.to_bits(), Ordering::Relaxed);
        }
    }

    /// Returns whether the audio sink is still active.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        !self.sink.empty()
    }
}

struct VehicleSource {
    controls: Arc<[AtomicU32; CONTROL_COUNT]>,
    phase: f32,
    texture_phase: f32,
    frame_sample: f32,
    right_channel: bool,
}

impl VehicleSource {
    fn new(controls: Arc<[AtomicU32; CONTROL_COUNT]>) -> Self {
        Self { controls, phase: 0.0, texture_phase: 0.0, frame_sample: 0.0, right_channel: false }
    }

    fn control(&self, index: usize) -> f32 {
        f32::from_bits(self.controls[index].load(Ordering::Relaxed))
    }

    fn next_frame(&mut self) -> f32 {
        let frequency = self.control(0).clamp(0.0, 400.0);
        let engine_gain = self.control(1).clamp(0.0, 1.0);
        let tyre_gain = self.control(2).clamp(0.0, 1.0);
        let skid_gain = self.control(3).clamp(0.0, 1.0);
        let surface_gain = self.control(4).clamp(0.0, 1.0);
        let impact_gain = self.control(5).clamp(0.0, 1.0);

        let engine = (self.phase.sin() + 0.32 * (self.phase * 2.0).sin()
            + 0.12 * (self.phase * 3.0).sin()) * engine_gain;
        let rolling = (self.texture_phase.sin() * 0.55
            + (self.texture_phase * 2.71).sin() * 0.25) * tyre_gain;
        let skid = (self.texture_phase * 7.3).sin() * skid_gain;
        let surface = ((self.texture_phase * 3.9).sin()
            + (self.texture_phase * 11.1).sin() * 0.35) * surface_gain;
        let impact = ((self.texture_phase * 19.0).sin()
            + (self.texture_phase * 31.0).sin() * 0.35) * impact_gain;

        self.phase = (self.phase + std::f32::consts::TAU * frequency / SAMPLE_RATE as f32)
            .rem_euclid(std::f32::consts::TAU);
        self.texture_phase = (self.texture_phase + 0.071).rem_euclid(std::f32::consts::TAU);
        ((engine * 0.52 + rolling * 0.12 + skid * 0.10 + surface * 0.08 + impact * 0.14) * 0.18)
            .clamp(-0.8, 0.8)
    }
}

impl Iterator for VehicleSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.right_channel {
            self.frame_sample = self.next_frame();
            self.right_channel = true;
            Some(self.frame_sample)
        } else {
            self.right_channel = false;
            Some(self.frame_sample)
        }
    }
}

impl Source for VehicleSource {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { 2 }
    fn sample_rate(&self) -> u32 { SAMPLE_RATE }
    fn total_duration(&self) -> Option<Duration> { None }
}
