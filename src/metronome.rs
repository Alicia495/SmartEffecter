use crate::state::SharedControls;
use std::sync::atomic::Ordering;

/// Sample-clock timing with no callback allocation.
pub struct Metronome {
    remaining: f64,
    beat: u32,
    age: u32,
    phase: f32,
    accent: bool,
    enabled: bool,
    gain: f32,
}
impl Default for Metronome {
    fn default() -> Self {
        Self {
            remaining: 0.,
            beat: 0,
            age: u32::MAX,
            phase: 0.,
            accent: false,
            enabled: false,
            gain: 0.,
        }
    }
}
impl Metronome {
    pub fn next(&mut self, rate: u32, controls: &SharedControls, muted: bool) -> f32 {
        let enabled = controls.metronome_enabled.load(Ordering::Relaxed) != 0;
        if enabled && !self.enabled {
            self.remaining = 0.;
            self.beat = 0;
        }
        self.enabled = enabled;
        if enabled {
            if self.remaining <= 0. {
                let beats = controls
                    .metronome_beats
                    .load(Ordering::Relaxed)
                    .clamp(1, 12);
                self.beat %= beats;
                self.accent = self.beat == 0;
                self.beat = (self.beat + 1) % beats;
                self.age = 0;
                self.phase = 0.;
                self.remaining += rate as f64 * 60.
                    / controls
                        .metronome_bpm
                        .load(Ordering::Relaxed)
                        .clamp(30, 300) as f64;
            }
            self.remaining -= 1.;
        }
        let target = if enabled && !muted {
            f32::from_bits(controls.metronome_gain.load(Ordering::Relaxed))
        } else {
            0.
        };
        self.gain += (target - self.gain) * 0.004;
        let length = (rate as f32 * 0.025) as u32;
        if self.age >= length {
            return 0.;
        }
        let t = self.age as f32 / rate as f32;
        let envelope = (1. - self.age as f32 / length as f32).powi(3) * (t / 0.001).min(1.);
        self.phase += std::f32::consts::TAU * if self.accent { 1760. } else { 1100. } / rate as f32;
        self.age += 1;
        self.phase.sin() * envelope * self.gain
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Tone;
    #[test]
    fn sample_clock_tempo_accent_stop_and_no_allocations() {
        for rate in [44100, 48000] {
            let controls = SharedControls::new(&Tone::default());
            controls.metronome_enabled.store(1, Ordering::Relaxed);
            let mut m = Metronome::default();
            let mut starts = Vec::with_capacity(10);
            let allocations = crate::allocation_check::count(|| {
                for i in 0..rate * 3 {
                    let v = m.next(rate, &controls, false);
                    assert!(v.is_finite() && v.abs() <= 0.126);
                    if m.age == 1 {
                        starts.push((i, m.accent));
                    }
                }
                controls.metronome_enabled.store(0, Ordering::Relaxed);
                for _ in 0..rate / 10 {
                    m.next(rate, &controls, false);
                }
                assert_eq!(m.next(rate, &controls, false), 0.);
            });
            assert_eq!(allocations, 0);
            assert_eq!(
                starts,
                (0..6)
                    .map(|i| (i * rate / 2, i % 4 == 0))
                    .collect::<Vec<_>>()
            );
        }
    }
}
