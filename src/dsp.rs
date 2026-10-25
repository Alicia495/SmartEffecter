use crate::state::Tone;
use std::f32::consts::TAU;

fn db(value: f32) -> f32 {
    10f32.powf(value / 20.)
}
fn pole(hz: f32, rate: f32) -> f32 {
    1. - (-TAU * hz / rate).exp()
}
fn mix(a: f32, b: f32, amount: f32) -> f32 {
    a + (b - a) * amount
}
#[derive(Default)]
struct LowPass {
    state: f32,
}
impl LowPass {
    fn run(&mut self, value: f32, alpha: f32) -> f32 {
        self.state += alpha * (value - self.state);
        self.state
    }
}
struct DelayLine {
    samples: Vec<f32>,
    head: usize,
}
impl DelayLine {
    fn new(size: usize) -> Self {
        Self {
            samples: vec![0.; size.max(4)],
            head: 0,
        }
    }
    fn read(&self, samples: f32) -> f32 {
        let delay = samples.clamp(1., (self.samples.len() - 2) as f32);
        let whole = delay as usize;
        let fraction = delay - whole as f32;
        // Wrap integer indices. Floating-point rem_euclid can round a tiny
        // negative position to exactly len, outside the allocated buffer.
        let newer = (self.head + self.samples.len() - whole) % self.samples.len();
        let older = (newer + self.samples.len() - 1) % self.samples.len();
        mix(self.samples[newer], self.samples[older], fraction)
    }
    fn push(&mut self, value: f32) {
        self.samples[self.head] = value;
        self.head = (self.head + 1) % self.samples.len();
    }
}
struct Comb {
    delay: DelayLine,
    damping: f32,
    length: f32,
}
impl Comb {
    fn new(seconds: f32, rate: f32) -> Self {
        let length = (seconds * rate).round();
        Self {
            delay: DelayLine::new(length as usize + 2),
            damping: 0.,
            length,
        }
    }
    fn run(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let old = self.delay.read(self.length);
        self.damping += damp * (old - self.damping);
        self.delay.push(input + self.damping * feedback);
        old
    }
}

pub struct Processor {
    rate: f32,
    target: Tone,
    current: Tone,
    tick: u32,
    wet: [f32; 7],
    mode: [f32; 7],
    enabled: [f32; 7],
    gate_envelope: f32,
    gate_gain: f32,
    comp_envelope: f32,
    drive_previous: f32,
    drive_filter: LowPass,
    bass_low: LowPass,
    amp_low: LowPass,
    amp_mid: LowPass,
    amp_dc: LowPass,
    cab1: LowPass,
    cab2: LowPass,
    chorus: DelayLine,
    phase: f32,
    delays: [DelayLine; 2],
    delay_time: f32,
    reverbs: [Vec<Comb>; 2],
    bypass_mix: f32,
    mute_mix: f32,
    startup: f32,
    input_gain: f32,
    output_gain: f32,
    threshold: f32,
    gate_release: f32,
    comp_attack: f32,
    drive_gain: f32,
    drive_tone: f32,
    drive_level: f32,
    amp_gain: f32,
    amp_eq: [f32; 3],
    amp_level: f32,
}
impl Processor {
    pub fn new(rate: f32, mut tone: Tone) -> Self {
        tone.sanitize();
        let mut result = Self {
            rate,
            target: tone,
            current: tone,
            tick: 0,
            wet: [0.; 7],
            mode: [0.; 7],
            enabled: std::array::from_fn(|i| if tone.effects[i].enabled { 1. } else { 0. }),
            gate_envelope: 0.,
            gate_gain: 0.,
            comp_envelope: 0.,
            drive_previous: 0.,
            drive_filter: LowPass::default(),
            bass_low: LowPass::default(),
            amp_low: LowPass::default(),
            amp_mid: LowPass::default(),
            amp_dc: LowPass::default(),
            cab1: LowPass::default(),
            cab2: LowPass::default(),
            chorus: DelayLine::new((rate * 0.06) as usize),
            phase: 0.,
            delays: std::array::from_fn(|_| DelayLine::new((rate * 1.6) as usize)),
            delay_time: tone.effects[5].params[0] * rate / 1000.,
            reverbs: std::array::from_fn(|channel| {
                [0.0297, 0.0371, 0.0411, 0.0437]
                    .iter()
                    .map(|time| Comb::new(time + channel as f32 * 0.0013, rate))
                    .collect()
            }),
            bypass_mix: 0.,
            mute_mix: 1.,
            startup: 0.,
            input_gain: 1.,
            output_gain: 1.,
            threshold: 0.,
            gate_release: 0.,
            comp_attack: 0.,
            drive_gain: 1.,
            drive_tone: 0.,
            drive_level: 1.,
            amp_gain: 1.,
            amp_eq: [1.; 3],
            amp_level: 1.,
        };
        result.wet = result.enabled;
        result.mode = std::array::from_fn(|i| tone.effects[i].mode as f32);
        result.coefficients();
        result
    }
    pub fn set_tone(&mut self, tone: Tone) {
        self.target = tone;
        self.enabled = std::array::from_fn(|i| if tone.effects[i].enabled { 1. } else { 0. });
    }
    fn coefficients(&mut self) {
        self.input_gain = db(self.current.input_db);
        self.output_gain = db(self.current.output_db);
        self.threshold = db(self.current.effects[0].params[0]);
        self.gate_release = (-1. / (self.rate * self.current.effects[0].params[1] / 1000.)).exp();
        self.comp_attack = (-1. / (self.rate * self.current.effects[1].params[1] / 1000.)).exp();
        self.drive_gain = 1. + self.current.effects[2].params[0] * 0.3;
        self.drive_tone = pole(700. + self.current.effects[2].params[1] * 65., self.rate);
        self.drive_level = db(self.current.effects[2].params[2]);
        self.amp_gain = 1. + self.current.effects[3].params[0] * 0.09;
        self.amp_eq = std::array::from_fn(|i| db(self.current.effects[3].params[i + 1]));
        self.amp_level = db(self.current.effects[3].params[4]);
    }
    /// Returns input after trim, then stereo output. No allocation, lock, or UI work.
    pub fn process(&mut self, raw: f32, bypass: bool, mute: bool) -> (f32, [f32; 2]) {
        if self.tick & 31 == 0 {
            for i in 0..7 {
                for j in 0..6 {
                    self.current.effects[i].params[j] = mix(
                        self.current.effects[i].params[j],
                        self.target.effects[i].params[j],
                        0.08,
                    );
                }
            }
            self.current.input_db = mix(self.current.input_db, self.target.input_db, 0.08);
            self.current.output_db = mix(self.current.output_db, self.target.output_db, 0.08);
            self.coefficients();
        }
        self.tick = self.tick.wrapping_add(1);
        let slew = 1. / (self.rate * 0.005);
        for i in 0..7 {
            self.wet[i] = approach(self.wet[i], self.enabled[i], slew);
            self.mode[i] = approach(self.mode[i], self.target.effects[i].mode as f32, slew);
        }
        self.bypass_mix = approach(self.bypass_mix, if bypass { 1. } else { 0. }, slew);
        self.mute_mix = approach(self.mute_mix, if mute { 0. } else { 1. }, slew);
        self.startup = (self.startup + 1. / (self.rate * 0.02)).min(1.);
        let input = if raw.is_finite() {
            raw * self.input_gain
        } else {
            0.
        };
        let mut x = input;
        self.gate_envelope = if x.abs() > self.gate_envelope {
            mix(self.gate_envelope, x.abs(), 0.15)
        } else {
            mix(x.abs(), self.gate_envelope, self.gate_release)
        };
        let gate_target = if self.gate_envelope > self.threshold {
            1.
        } else {
            0.
        };
        self.gate_gain = mix(
            self.gate_gain,
            gate_target,
            if gate_target > 0. {
                0.02
            } else {
                1. - self.gate_release
            },
        );
        x = mix(x, x * self.gate_gain, self.wet[0]);
        let attack = if x.abs() > self.comp_envelope {
            self.comp_attack
        } else {
            (-1. / (self.rate * 0.08)).exp()
        };
        self.comp_envelope = mix(x.abs(), self.comp_envelope, attack);
        let amount = self.current.effects[1].params[0] / 100.;
        let compression = if self.comp_envelope > 0.12 {
            (self.comp_envelope / 0.12).powf(-(0.85 * amount))
        } else {
            1.
        };
        x = mix(
            x,
            x * compression * db(self.current.effects[1].params[2]),
            self.wet[1],
        );
        let shape = |v: f32| {
            let drive = v * self.drive_gain;
            mix(drive.tanh(), (drive * 1.4).clamp(-1., 1.), self.mode[2])
        };
        let driven = (shape((self.drive_previous + x) * 0.5) + shape(x)) * 0.5;
        self.drive_previous = x;
        let driven = self.drive_filter.run(driven, self.drive_tone) * self.drive_level;
        let bass_low = self.bass_low.run(x, pole(180., self.rate));
        let driven = if self.target.bass {
            driven * 0.55 + bass_low * 0.8
        } else {
            driven
        };
        x = mix(x, driven, self.wet[2]);
        let low = self.amp_low.run(
            x,
            pole(if self.target.bass { 120. } else { 220. }, self.rate),
        );
        let low_mid = self.amp_mid.run(
            x,
            pole(if self.target.bass { 900. } else { 1800. }, self.rate),
        );
        let eq = low * self.amp_eq[0]
            + (low_mid - low) * self.amp_eq[1]
            + (x - low_mid) * self.amp_eq[2];
        let pre = eq * self.amp_gain;
        let amp = mix(
            pre / (1. + 0.15 * pre.abs()),
            (pre * 2.2).tanh() * 0.7,
            self.mode[3],
        );
        let amp = amp
            - self.amp_dc.run(
                amp,
                pole(if self.target.bass { 25. } else { 70. }, self.rate),
            );
        let amp = self.cab1.run(amp, pole(4800., self.rate));
        let amp = self.cab2.run(amp, pole(4800., self.rate)) * self.amp_level;
        x = mix(x, amp, self.wet[3]);
        self.phase = (self.phase + TAU * self.current.effects[4].params[0] / self.rate) % TAU;
        let depth = self.current.effects[4].params[1] * 0.00007 * self.rate;
        let chorus_wet = self.current.effects[4].params[2] / 100. * self.wet[4];
        let mut stereo = std::array::from_fn(|i| {
            mix(
                x,
                self.chorus
                    .read(self.rate * 0.014 + (self.phase + i as f32 * 1.3).sin() * depth),
                chorus_wet,
            )
        });
        self.chorus.push(x);
        self.delay_time = mix(
            self.delay_time,
            self.current.effects[5].params[0] * self.rate / 1000.,
            0.0005,
        );
        let delay_mix = self.current.effects[5].params[2] / 100. * self.wet[5];
        let feedback = self.current.effects[5].params[1] / 100.;
        for (i, sample) in stereo.iter_mut().enumerate() {
            let delayed = self.delays[i].read(self.delay_time);
            self.delays[i].push(*sample + delayed * feedback);
            *sample = mix(*sample, delayed, delay_mix);
        }
        let reverb_mix = self.current.effects[6].params[2] / 100. * self.wet[6];
        let feedback = 0.45 + self.current.effects[6].params[0] * 0.0043;
        let damp = 0.08 + self.current.effects[6].params[1] * 0.006;
        for (i, sample) in stereo.iter_mut().enumerate() {
            let mut reflected = 0.;
            for comb in &mut self.reverbs[i] {
                reflected += comb.run(*sample * 0.18, feedback, damp);
            }
            *sample = mix(*sample, reflected, reverb_mix);
            *sample = mix(*sample, input, self.bypass_mix)
                * self.output_gain
                * self.mute_mix
                * self.startup;
            if !sample.is_finite() {
                *sample = 0.;
            }
        }
        (input, stereo)
    }
}
fn approach(value: f32, target: f32, max_delta: f32) -> f32 {
    value + (target - value).clamp(-max_delta, max_delta)
}

pub fn pitch(samples: &[f32], rate: f32) -> Option<(String, f32, f32)> {
    if samples.len() < 1024 {
        return None;
    }
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    let rms =
        (samples.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / samples.len() as f32).sqrt();
    if rms < 0.004 {
        return None;
    }
    let min = (rate / 1400.) as usize;
    let max = ((rate / 25.) as usize).min(samples.len() / 2 - 2);
    let score = |lag: usize| {
        let mut sum = 0.;
        let mut norm = 0.;
        for i in 0..samples.len() - max {
            let a = samples[i] - mean;
            let b = samples[i + lag] - mean;
            sum += a * b;
            norm += a * a + b * b;
        }
        2. * sum / norm.max(1e-12)
    };
    let mut best = (0, 0f32);
    for lag in min + 1..max {
        let s = score(lag);
        if s > 0.82 && s > score(lag - 1) && s >= score(lag + 1) {
            best = (lag, s);
            break;
        }
        if s > best.1 {
            best = (lag, s);
        }
    }
    if best.0 == 0 || best.1 < 0.82 {
        return None;
    }
    let (lag, b) = best;
    let a = score(lag - 1);
    let c = score(lag + 1);
    let denom = a - 2. * b + c;
    let delta = if denom.abs() > 1e-7 {
        (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
    } else {
        0.
    };
    let hz = rate / (lag as f32 + delta);
    let midi = 69. + 12. * (hz / 440.).log2();
    let note = midi.round() as i32;
    let names = [
        "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
    ];
    Some((
        format!("{}{}", names[note.rem_euclid(12) as usize], note / 12 - 1),
        (midi - note as f32) * 100.,
        hz,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tuner_tracks_five_string_bass_at_both_rates() {
        for rate in [11025., 12000.] {
            for (hz, expected) in [
                (30.8677, "B0"),
                (41.2034, "E1"),
                (55., "A1"),
                (73.4162, "D2"),
                (97.9989, "G2"),
            ] {
                let signal: Vec<_> = (0..2048)
                    .map(|i| {
                        let phase = TAU * hz * i as f32 / rate;
                        0.2 * (phase.sin() + 0.4 * (2. * phase).sin() + 0.2 * (3. * phase).sin())
                    })
                    .collect();
                let (note, cents, _) = pitch(&signal, rate).unwrap();
                assert_eq!(note, expected);
                assert!(cents.abs() < 2., "{expected}: {cents}");
            }
        }
    }
    #[test]
    fn bass_preserves_low_b_fundamental_and_runs_without_allocations() {
        for drive in [false, true] {
            let mut tone = Tone::bass_factory(drive);
            tone.effects[0].enabled = false;
            tone.effects[1].enabled = false;
            let mut bass = Processor::new(48000., tone);
            tone.bass = false;
            let mut guitar = Processor::new(48000., tone);
            let mut energy = [0f32; 2];
            let allocations = crate::allocation_check::count(|| {
                for i in 0..48000 {
                    let raw = 0.1 * (TAU * 30.87 * i as f32 / 48000.).sin();
                    let (_, b) = bass.process(raw, false, false);
                    let (_, g) = guitar.process(raw, false, false);
                    assert!(b.iter().all(|s| s.is_finite()));
                    if i > 24000 {
                        energy[0] += b[0] * b[0];
                        energy[1] += g[0] * g[0];
                    }
                }
            });
            assert_eq!(allocations, 0);
            assert!(energy[0] > energy[1] * 1.5, "{energy:?}");
        }
    }
    #[test]
    fn fractional_delay_wraps_at_rounding_boundary() {
        let mut line = DelayLine::new(2880);
        line.head = 672;
        line.samples[0] = 0.75;
        line.samples[2879] = 0.25;
        // Formerly rem_euclid(2880.0) rounded this position to 2880.0.
        let output = line.read(672.000_06);
        assert!((output - 0.75).abs() < 0.0001);
        assert!((line.read(672.5) - 0.5).abs() < 0.0001);
        for head in 0..2880 {
            line.head = head;
            assert!(line.read(head as f32 + 0.000_06).is_finite());
        }
    }
    fn dry() -> Tone {
        let mut t = Tone::default();
        for e in &mut t.effects {
            e.enabled = false;
        }
        t.output_db = 0.;
        t
    }
    #[test]
    fn dry_signal_has_no_dsp_sample_delay() {
        let mut p = Processor::new(48000., dry());
        for _ in 0..2000 {
            p.process(0., false, false);
        }
        let (_, y) = p.process(0.3, false, false);
        assert!((y[0] - 0.3).abs() < 1e-5);
        assert!((y[1] - 0.3).abs() < 1e-5);
    }
    #[test]
    fn delay_impulse_arrives_at_requested_time() {
        let mut t = dry();
        t.effects[5].enabled = true;
        t.effects[5].params = [100., 0., 100., 0., 0., 0.];
        let mut p = Processor::new(48000., t);
        let mut peak = (0, 0f32);
        for i in 0..6000 {
            let (_, y) = p.process(if i == 0 { 0.5 } else { 0. }, false, false);
            if y[0].abs() > peak.1 {
                peak = (i, y[0].abs());
            }
        }
        assert_eq!(peak.0, 4800);
        assert!((peak.1 - 0.5).abs() < 1e-4);
    }
    #[test]
    fn mute_and_bypass_keep_output_gain() {
        let mut t = Tone::factory(2);
        t.output_db = -6.;
        let mut p = Processor::new(48000., t);
        for _ in 0..5000 {
            p.process(0.25, true, false);
        }
        let (_, y) = p.process(0.25, true, false);
        assert!((y[0] - 0.25 * db(-6.)).abs() < 1e-4);
        for _ in 0..1000 {
            p.process(0.25, true, true);
        }
        assert_eq!(p.process(0.25, true, true).1, [0., 0.]);
    }
    #[test]
    fn all_effects_stay_finite_under_extreme_settings() {
        let mut t = Tone::factory(2);
        for (i, e) in t.effects.iter_mut().enumerate() {
            e.enabled = true;
            for (j, s) in crate::state::specs(i).iter().enumerate() {
                e.params[j] = s.max;
            }
        }
        let mut p = Processor::new(48000., t);
        for i in 0..96000 {
            let (_, y) = p.process((i as f32 * 0.3).sin() * 0.9, false, false);
            assert!(y.iter().all(|v| v.is_finite() && v.abs() < 20.));
        }
    }
    #[test]
    fn tuner_finds_a2_and_rejects_silence() {
        let signal: Vec<_> = (0..2048)
            .map(|i| (TAU * 110. * i as f32 / 12000.).sin() * 0.2)
            .collect();
        let (note, cents, hz) = pitch(&signal, 12000.).unwrap();
        assert_eq!(note, "A2");
        assert!(cents.abs() < 1.);
        assert!((hz - 110.).abs() < 0.1);
        assert!(pitch(&vec![0.; 2048], 12000.).is_none());
    }

    fn sine_energy(effect: usize, amplitude: f32, hz: f32) -> (f32, f32) {
        let mut tone = dry();
        tone.effects[effect].enabled = true;
        if effect == 1 {
            tone.effects[1].params[0] = 100.;
        }
        let mut processor = Processor::new(48000., tone);
        let mut energy = [0.; 2];
        for i in 0..48000 {
            let input = amplitude * (TAU * hz * i as f32 / 48000.).sin();
            let (_, output) = processor.process(input, false, false);
            if i >= 24000 {
                energy[0] += input * input;
                energy[1] += output[0] * output[0];
            }
        }
        (energy[0], energy[1])
    }
    #[test]
    fn gate_suppresses_quiet_input_and_passes_notes() {
        let (dry, quiet) = sine_energy(0, 0.0001, 110.);
        assert!(quiet < dry * 0.001);
        let (dry, note) = sine_energy(0, 0.2, 110.);
        assert!(note > dry * 0.95);
    }
    #[test]
    fn compressor_reduces_loud_signal_without_lookahead() {
        let (dry, compressed) = sine_energy(1, 0.7, 220.);
        assert!(compressed < dry * 0.5);
        assert!(compressed > 0.);
    }
    #[test]
    fn cabinet_attenuates_high_frequencies() {
        let (_, low) = sine_energy(3, 0.02, 440.);
        let (_, high) = sine_energy(3, 0.02, 12000.);
        assert!(high < low * 0.15);
    }
    #[test]
    fn chorus_produces_stereo_and_reverb_has_a_tail() {
        let mut tone = dry();
        tone.effects[4].enabled = true;
        let mut processor = Processor::new(48000., tone);
        let mut difference = 0.;
        for i in 0..48000 {
            let (_, out) =
                processor.process((TAU * 220. * i as f32 / 48000.).sin() * 0.1, false, false);
            difference += (out[0] - out[1]).abs();
        }
        assert!(difference > 1.);

        let mut tone = dry();
        tone.effects[6].enabled = true;
        tone.effects[6].params[2] = 100.;
        let mut processor = Processor::new(48000., tone);
        let mut tail_energy = 0.;
        for i in 0..24000 {
            let (_, out) = processor.process(if i == 0 { 0.5 } else { 0. }, false, false);
            if i > 1000 {
                tail_energy += out[0] * out[0];
            }
        }
        assert!(tail_energy > 0.0001);
    }
    #[test]
    fn tuner_tracks_six_open_strings_at_both_rates() {
        for rate in [11025., 12000.] {
            for (hz, expected) in [
                (82.4069, "E2"),
                (110., "A2"),
                (146.8324, "D3"),
                (195.9977, "G3"),
                (246.9417, "B3"),
                (329.6276, "E4"),
            ] {
                let signal: Vec<_> = (0..2048)
                    .map(|i| {
                        let phase = TAU * hz * i as f32 / rate;
                        0.2 * (phase.sin() + 0.4 * (2. * phase).sin() + 0.2 * (3. * phase).sin())
                    })
                    .collect();
                let (note, cents, _) = pitch(&signal, rate).unwrap();
                assert_eq!(note, expected);
                assert!(cents.abs() < 2., "{expected}: {cents}");
            }
        }
    }
}
