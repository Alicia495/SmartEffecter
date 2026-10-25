use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

pub const EFFECT_NAMES: [&str; 7] = ["GATE", "COMP", "DRIVE", "AMP", "CHORUS", "DELAY", "REVERB"];
pub const EFFECT_HINTS: [&str; 7] = [
    "静かなところを、静かに。",
    "弾き心地をそろえる。",
    "歪みの表情をつくる。",
    "音の芯と輪郭。",
    "音にゆらぎと広がり。",
    "フレーズに余韻を。",
    "空間の響きを添える。",
];

#[derive(Clone, Copy)]
pub struct ParamSpec {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub unit: &'static str,
    pub step: f32,
}
const fn p(
    name: &'static str,
    min: f32,
    max: f32,
    default: f32,
    unit: &'static str,
    step: f32,
) -> ParamSpec {
    ParamSpec {
        name,
        min,
        max,
        default,
        unit,
        step,
    }
}
pub fn specs(effect: usize) -> &'static [ParamSpec] {
    match effect {
        0 => {
            const {
                &[
                    p("Threshold", -80., -10., -55., "dB", 1.),
                    p("Release", 10., 500., 120., "ms", 5.),
                ]
            }
        }
        1 => {
            const {
                &[
                    p("Amount", 0., 100., 35., "%", 1.),
                    p("Attack", 1., 100., 15., "ms", 1.),
                    p("Level", -12., 12., 0., "dB", 0.5),
                ]
            }
        }
        2 => {
            const {
                &[
                    p("Gain", 0., 100., 35., "%", 1.),
                    p("Tone", 0., 100., 55., "%", 1.),
                    p("Level", -24., 6., -6., "dB", 0.5),
                ]
            }
        }
        3 => {
            const {
                &[
                    p("Gain", 0., 100., 25., "%", 1.),
                    p("Bass", -12., 12., 0., "dB", 0.5),
                    p("Mid", -12., 12., 0., "dB", 0.5),
                    p("Treble", -12., 12., 0., "dB", 0.5),
                    p("Level", -24., 6., -3., "dB", 0.5),
                ]
            }
        }
        4 => {
            const {
                &[
                    p("Rate", 0.1, 5., 0.8, "Hz", 0.1),
                    p("Depth", 0., 100., 35., "%", 1.),
                    p("Mix", 0., 100., 25., "%", 1.),
                ]
            }
        }
        5 => {
            const {
                &[
                    p("Time", 20., 1500., 350., "ms", 5.),
                    p("Feedback", 0., 90., 30., "%", 1.),
                    p("Mix", 0., 100., 18., "%", 1.),
                ]
            }
        }
        _ => {
            const {
                &[
                    p("Size", 0., 100., 40., "%", 1.),
                    p("Tone", 0., 100., 55., "%", 1.),
                    p("Mix", 0., 100., 15., "%", 1.),
                ]
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Effect {
    pub enabled: bool,
    pub params: [f32; 6],
    pub mode: u32,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Tone {
    #[serde(default)]
    pub bass: bool,
    pub effects: [Effect; 7],
    pub input_db: f32,
    pub output_db: f32,
}
impl Default for Tone {
    fn default() -> Self {
        Self {
            bass: false,
            effects: std::array::from_fn(|i| {
                let mut params = [0.; 6];
                for (j, spec) in specs(i).iter().enumerate() {
                    params[j] = spec.default;
                }
                Effect {
                    enabled: matches!(i, 0 | 3 | 6),
                    params,
                    mode: 0,
                }
            }),
            input_db: 0.,
            output_db: -12.,
        }
    }
}
impl Tone {
    pub fn factory(index: usize) -> Self {
        let mut tone = Self::default();
        if index > 0 {
            tone.effects[2].enabled = true;
            tone.effects[2].params[0] = if index == 1 { 32. } else { 65. };
            tone.effects[3].mode = 1;
            tone.effects[3].params[0] = if index == 1 { 25. } else { 42. };
        }
        if index == 2 {
            tone.effects[1].enabled = true;
            tone.effects[5].enabled = true;
        }
        tone
    }
    pub fn bass_factory(drive: bool) -> Self {
        let mut tone = Self {
            bass: true,
            ..Self::default()
        };
        tone.effects[0].params[0] = -65.;
        tone.effects[1].enabled = true;
        tone.effects[1].params[0] = 55.;
        tone.effects[2].enabled = drive;
        tone.effects[2].params[0] = 25.;
        tone.effects[3].params[0] = 12.;
        tone.effects[3].params[1] = 2.;
        tone.effects[6].enabled = false;
        tone
    }
    pub fn sanitize(&mut self) {
        fn safe(v: f32, min: f32, max: f32, default: f32) -> f32 {
            if v.is_finite() {
                v.clamp(min, max)
            } else {
                default
            }
        }
        self.input_db = safe(self.input_db, -24., 24., 0.);
        self.output_db = safe(self.output_db, -60., 0., -12.);
        for (i, effect) in self.effects.iter_mut().enumerate() {
            effect.mode = effect.mode.min(1);
            for (j, spec) in specs(i).iter().enumerate() {
                effect.params[j] = safe(effect.params[j], spec.min, spec.max, spec.default);
            }
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub tone: Tone,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema: u32,
    pub tone: Tone,
    pub device: String,
    /// Empty uses the input ASIO device; otherwise a WASAPI endpoint ID.
    pub output_device: String,
    pub sample_rate: u32,
    pub buffer: u32,
    pub input_channel: usize,
    /// Empty disables, "asio" uses another channel, otherwise a WASAPI ID.
    pub mic_device: String,
    pub mic_channel: usize,
    pub instrument_db: f32,
    pub mic_db: f32,
    pub presets: Vec<Preset>,
    pub waveform: bool,
    pub dark: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: 1,
            tone: Tone::default(),
            device: String::new(),
            output_device: String::new(),
            sample_rate: 48000,
            buffer: 32,
            input_channel: 1,
            mic_device: String::new(),
            mic_channel: 0,
            instrument_db: 0.,
            mic_db: -12.,
            presets: vec![],
            waveform: true,
            dark: true,
        }
    }
}
pub fn data_dir() -> PathBuf {
    std::env::var_os("GUITAR_EFFECTER_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join("GuitarSmartEffecter")
        })
}
impl Settings {
    pub fn backup_unreadable() -> Result<PathBuf, String> {
        let source = data_dir().join("settings.json");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let backup = data_dir().join(format!("settings.unreadable-{stamp}.json"));
        std::fs::copy(source, &backup).map_err(|e| format!("設定の退避に失敗: {e}"))?;
        Ok(backup)
    }
    pub fn load() -> Result<Self, String> {
        let path = data_dir().join("settings.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut settings: Self =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if settings.schema != 1 {
            return Err("設定ファイルのバージョンが未対応です".into());
        }
        settings.tone.sanitize();
        settings.sample_rate = if matches!(settings.sample_rate, 44100 | 48000) {
            settings.sample_rate
        } else {
            48000
        };
        settings.buffer = settings.buffer.clamp(16, 8192);
        settings.input_channel = settings.input_channel.min(63);
        settings.mic_channel = settings.mic_channel.min(63);
        for gain in [&mut settings.instrument_db, &mut settings.mic_db] {
            *gain = if gain.is_finite() {
                gain.clamp(-60., 6.)
            } else {
                -12.
            };
        }
        settings.presets.truncate(128);
        for preset in &mut settings.presets {
            preset.tone.sanitize();
            preset.name = preset.name.chars().take(40).collect();
        }
        Ok(settings)
    }
    pub fn save(&self) -> Result<(), String> {
        let dir = data_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        use std::io::Write;
        let temp = dir.join("settings.json.tmp");
        let mut file = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
        file.write_all(&data)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(temp, dir.join("settings.json")).map_err(|e| e.to_string())
    }
}

pub struct SharedControls {
    epoch: AtomicU32,
    values: [AtomicU32; 52],
    pub instrument_gain: AtomicU32,
    pub mic_gain: AtomicU32,
    pub mute: AtomicU32,
    pub bypass: AtomicU32,
}
impl SharedControls {
    pub fn new(tone: &Tone) -> Self {
        let shared = Self {
            epoch: AtomicU32::new(0),
            values: std::array::from_fn(|_| AtomicU32::new(0)),
            instrument_gain: AtomicU32::new(1f32.to_bits()),
            mic_gain: AtomicU32::new(0.25118864f32.to_bits()),
            mute: AtomicU32::new(0),
            bypass: AtomicU32::new(0),
        };
        shared.publish(tone);
        shared
    }
    pub fn publish(&self, tone: &Tone) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        for i in 0..7 {
            for j in 0..6 {
                self.values[i * 6 + j].store(tone.effects[i].params[j].to_bits(), Ordering::SeqCst);
            }
            self.values[42 + i].store(
                u32::from(tone.effects[i].enabled) | (tone.effects[i].mode << 1),
                Ordering::SeqCst,
            );
        }
        self.values[51].store(u32::from(tone.bass), Ordering::SeqCst);
        self.values[49].store(tone.input_db.to_bits(), Ordering::SeqCst);
        self.values[50].store(tone.output_db.to_bits(), Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }
    // Bounded snapshot: a concurrent UI edit skips this block, never spins in the audio thread.
    pub fn snapshot(&self, previous: &mut Tone) {
        let before = self.epoch.load(Ordering::SeqCst);
        if before & 1 != 0 {
            return;
        }
        let mut next = *previous;
        for i in 0..7 {
            for j in 0..6 {
                next.effects[i].params[j] =
                    f32::from_bits(self.values[i * 6 + j].load(Ordering::SeqCst));
            }
            let flags = self.values[42 + i].load(Ordering::SeqCst);
            next.effects[i].enabled = flags & 1 != 0;
            next.effects[i].mode = flags >> 1;
        }
        next.bass = self.values[51].load(Ordering::SeqCst) != 0;
        next.input_db = f32::from_bits(self.values[49].load(Ordering::SeqCst));
        next.output_db = f32::from_bits(self.values[50].load(Ordering::SeqCst));
        if self.epoch.load(Ordering::SeqCst) == before {
            *previous = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn older_settings_without_mixer_or_bass_still_load() {
        let mut json = serde_json::to_value(Settings::default()).unwrap();
        for key in ["mic_device", "mic_channel", "mic_db", "instrument_db"] {
            json.as_object_mut().unwrap().remove(key);
        }
        json["tone"].as_object_mut().unwrap().remove("bass");
        let settings: Settings = serde_json::from_value(json).unwrap();
        assert!(!settings.tone.bass);
        assert!(settings.mic_device.is_empty());
        assert_eq!(settings.instrument_db, 0.);
    }
    #[test]
    fn corrupt_values_are_bounded() {
        let mut t = Tone {
            output_db: f32::NAN,
            ..Tone::default()
        };
        t.effects[5].params[1] = 200.;
        t.sanitize();
        assert_eq!(t.output_db, -12.);
        assert_eq!(t.effects[5].params[1], 90.);
    }
    #[test]
    fn settings_round_trip_preserves_user_tone() {
        let mut s = Settings {
            tone: Tone::factory(2),
            ..Settings::default()
        };
        s.presets.push(Preset {
            name: "リード".into(),
            tone: s.tone,
        });
        let json = serde_json::to_string(&s).unwrap();
        let copy: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(copy.presets[0].name, "リード");
        assert_eq!(copy.tone.effects[2].params[0], 65.);
    }
    #[test]
    fn parameter_snapshot_is_consistent() {
        let original = Tone::default();
        let shared = SharedControls::new(&original);
        let expected = Tone::factory(2);
        shared.publish(&expected);
        let mut actual = original;
        shared.snapshot(&mut actual);
        assert_eq!(actual.effects[2].params[0], expected.effects[2].params[0]);
        assert!(actual.effects[5].enabled);
    }
}
