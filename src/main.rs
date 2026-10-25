#![cfg_attr(all(target_os = "windows", not(test)), windows_subsystem = "windows")]
mod app;
mod audio;
mod dsp;
mod state;
slint::include_modules!();

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let _ = std::fs::create_dir_all(state::data_dir());
        let _ = std::fs::write(state::data_dir().join("last-panic.txt"), info.to_string());
    }));
    if let Err(error) = entry() {
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "{error}");
        let _ = std::fs::create_dir_all(state::data_dir());
        let _ = std::fs::write(state::data_dir().join("last-error.txt"), error.to_string());
        std::process::exit(1);
    }
}
fn argument<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}
fn report(
    args: &[String],
    value: &impl serde::Serialize,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let json = serde_json::to_string_pretty(value)?;
    if let Some(path) = argument(args, "--report") {
        std::fs::write(path, &json)?;
    }
    let _ = writeln!(std::io::stdout(), "{json}");
    Ok(())
}
fn entry() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        return report(
            &args,
            &serde_json::json!({"name":"SmartEffecter","version":env!("CARGO_PKG_VERSION"),"target":"windows-x64"}),
        );
    }
    if args.iter().any(|a| a == "--devices") {
        return report(&args, &audio::devices().map_err(std::io::Error::other)?);
    }
    if args.iter().any(|a| a == "--outputs") {
        return report(
            &args,
            &audio::windows_outputs().map_err(std::io::Error::other)?,
        );
    }
    if args.iter().any(|a| a == "--inputs") {
        return report(
            &args,
            &audio::windows_inputs().map_err(std::io::Error::other)?,
        );
    }
    if let Some(seconds) = argument(&args, "--stress-dsp") {
        let seconds = seconds.parse::<u64>()?.clamp(1, 7200);
        let start = std::time::Instant::now();
        let mut tone = state::Tone::factory(2);
        for effect in &mut tone.effects {
            effect.enabled = true;
        }
        let mut processor = dsp::Processor::new(48000., tone);
        let mut peak = 0f32;
        for i in 0..seconds * 48000 {
            if i % 48000 == 0 {
                let second = i / 48000;
                tone.effects[2].params[0] = (second % 101) as f32;
                tone.effects[4].params[0] = 0.1 + (second % 50) as f32 * 0.1;
                tone.effects[4].params[1] = (second % 101) as f32;
                tone.effects[5].params[0] = 20. + (second % 297) as f32 * 5.;
                processor.set_tone(tone);
            }
            // Plucked harmonic signal alternating with silence, including tails.
            let t = (i % 48000) as f32 / 48000.;
            let phase = std::f32::consts::TAU * 110. * t;
            let input = if i / 48000 % 8 < 4 {
                0.2 * (phase.sin() + 0.3 * (2. * phase).sin()) * (-t * 5.).exp()
            } else {
                0.
            };
            let (_, output) = processor.process(input, false, false);
            if output.iter().any(|v| !v.is_finite()) {
                return Err("Non-finite stress output".into());
            }
            peak = peak.max(output[0].abs()).max(output[1].abs());
        }
        return report(
            &args,
            &serde_json::json!({"passed":true,"simulated_audio_seconds":seconds,"wall_seconds":start.elapsed().as_secs_f64(),"frames":seconds*48000,"peak":peak,"all_effects":true,"parameter_changes":true}),
        );
    }
    if let Some(seconds) = argument(&args, "--probe") {
        use std::{
            sync::{Arc, atomic::Ordering},
            time::{Duration, Instant},
        };
        let seconds = seconds.parse::<u64>()?.clamp(1, 3600);
        let devices = audio::devices().map_err(std::io::Error::other)?;
        let device = devices
            .iter()
            .find(|d| d.name.contains("M-Track"))
            .or_else(|| devices.first())
            .ok_or("No ASIO device found")?;
        let mut settings = state::Settings {
            device: device.name.clone(),
            ..state::Settings::default()
        };
        if let Some(output) = argument(&args, "--output") {
            settings.output_device = output.to_owned();
        }
        if let Some(size) = argument(&args, "--buffer") {
            settings.buffer = size.parse()?;
        }
        if let Some(rate) = argument(&args, "--rate") {
            settings.sample_rate = rate.parse()?;
        }
        if let Some(channel) = argument(&args, "--input") {
            settings.input_channel = channel.parse::<usize>()?.saturating_sub(1);
        }
        if args.iter().any(|a| a == "--all-effects") {
            for effect in &mut settings.tone.effects {
                effect.enabled = true;
            }
        }
        if let Some(monitor) = argument(&args, "--monitor-output") {
            settings.monitor_device = monitor.to_owned();
        }
        if let Some(mic) = argument(&args, "--mic") {
            settings.mic_device = mic.to_owned();
        }
        if let Some(channel) = argument(&args, "--mic-channel") {
            settings.mic_channel = channel.parse::<usize>()?.saturating_sub(1);
        }
        settings.monitor_enabled = !args.iter().any(|a| a == "--no-monitor");
        let controls = Arc::new(state::SharedControls::new(&settings.tone));
        controls.mute.store(1, Ordering::Relaxed);
        let mut session =
            audio::Session::open(&settings, controls.clone()).map_err(std::io::Error::other)?;
        let mut feed = session.feed.take().unwrap();
        let start = Instant::now();
        let mut peak = 0f32;
        let mut points = 0u64;
        while start.elapsed() < Duration::from_secs(seconds) {
            while let Ok(point) = feed.wave.pop() {
                peak = peak.max(point.high_in.abs()).max(point.low_in.abs());
                points += 1;
            }
            while feed.tuner.pop().is_ok() {}
            if args.iter().any(|a| a == "--stress-controls") {
                let mut tone = settings.tone;
                let t = start.elapsed().as_secs_f32();
                tone.effects[2].params[0] = (t * 2.).sin() * 40. + 50.;
                tone.effects[5].params[0] = 200. + (t * 0.7).sin() * 100.;
                controls.publish(&tone);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let value = serde_json::json!({"device":settings.device,"output_device":settings.output_device,"route":session.info,"sample_rate":settings.sample_rate,"requested_buffer":settings.buffer,"duration_seconds":start.elapsed().as_secs_f64(),"all_effects":args.iter().any(|a|a=="--all-effects"),"muted_output":true,"input_peak":peak,"wave_points_received":points,"metrics":session.metrics.report(),"monitor_device":settings.monitor_device,"monitor_metrics":session.monitor_metrics.as_ref().map(|m|m.report())});
        drop(session);
        return report(&args, &value);
    }
    if args.iter().any(|a| a == "--render-chords") {
        use std::f32::consts::TAU;
        let out = argument(&args, "--out").unwrap_or("artifacts/chords");
        std::fs::create_dir_all(out)?;
        let saved = state::Settings::load()?.tone;
        let mut reports = vec![];
        for (name, tone) in [
            ("current", saved),
            ("bass-clean", state::Tone::bass_factory(false)),
            ("bass-drive", state::Tone::bass_factory(true)),
        ] {
            let mut processor = dsp::Processor::new(48000., tone);
            let mut writer = hound::WavWriter::create(
                format!("{out}/{name}.wav"),
                hound::WavSpec {
                    channels: 2,
                    sample_rate: 48000,
                    bits_per_sample: 24,
                    sample_format: hound::SampleFormat::Int,
                },
            )?;
            let mut peak = 0f32;
            let mut clipped = 0u64;
            let start = std::time::Instant::now();
            for i in 0..48000 * 6 {
                let t = i as f32 / 48000.;
                let notes = [82.4069, 103.8262, 123.4708];
                let chord = (48000 * 2..48000 * 4).contains(&i);
                let mut input = 0.;
                if i < 48000 * 4 {
                    for (n, hz) in notes.iter().enumerate() {
                        if n > 0 && !chord {
                            continue;
                        }
                        let phase = TAU * hz * t;
                        input += 0.14
                            * (phase.sin() + 0.3 * (2. * phase).sin() + 0.15 * (3. * phase).sin())
                            * (-3. * (t % 0.5)).exp();
                    }
                }
                let (_, output) = processor.process(input, false, false);
                for sample in output {
                    if !sample.is_finite() {
                        return Err("non-finite chord fixture".into());
                    }
                    peak = peak.max(sample.abs());
                    if sample.abs() > 1. {
                        clipped += 1;
                    }
                    writer.write_sample((sample.clamp(-1., 1.) * 8_388_607.) as i32)?;
                }
            }
            writer.finalize()?;
            reports.push(serde_json::json!({"preset":name,"peak":peak,"clipped_samples":clipped,"audio_seconds":6,"render_seconds":start.elapsed().as_secs_f64()}));
        }
        return report(&args, &reports);
    }
    if args.iter().any(|a| a == "--render-fixture") {
        use std::f32::consts::TAU;
        let out = argument(&args, "--out").unwrap_or("artifacts");
        std::fs::create_dir_all(out)?;
        let mut measurements = vec![];
        for index in 0..3 {
            let name = ["clean", "crunch", "lead"][index];
            let mut processor = dsp::Processor::new(48000., state::Tone::factory(index));
            let mut writer = hound::WavWriter::create(
                format!("{out}/{name}.wav"),
                hound::WavSpec {
                    channels: 2,
                    sample_rate: 48000,
                    bits_per_sample: 24,
                    sample_format: hound::SampleFormat::Int,
                },
            )?;
            let start = std::time::Instant::now();
            let mut peak = 0f32;
            for i in 0..48000 * 8 {
                let t = i as f32 / 48000.;
                let notes = [82.4069, 110., 146.832, 196.];
                let hz = notes[(i / 24000) % 4];
                let envelope = (-(t % 0.5) * 7.).exp();
                let dry = (0.22 * (TAU * hz * t).sin()
                    + 0.09 * (TAU * hz * 2. * t).sin()
                    + 0.04 * (TAU * hz * 3. * t).sin())
                    * envelope;
                let (_, output) = processor.process(dry, false, false);
                for sample in output {
                    if !sample.is_finite() {
                        return Err("DSP produced a non-finite sample".into());
                    }
                    peak = peak.max(sample.abs());
                    writer.write_sample((sample.clamp(-1., 1.) * 8_388_607.) as i32)?;
                }
            }
            writer.finalize()?;
            measurements.push(serde_json::json!({"preset":name,"peak":peak,"render_seconds":start.elapsed().as_secs_f64(),"audio_seconds":8}));
        }
        return report(&args, &measurements);
    }
    app::run(&args)
}

#[cfg(test)]
mod allocation_check {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };
    thread_local! {static TRACK:Cell<bool>=const{Cell::new(false)};static COUNT:Cell<usize>=const{Cell::new(0)};}
    pub struct Allocator;
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let _ = TRACK.try_with(|t| {
                if t.get() {
                    COUNT.with(|c| c.set(c.get() + 1));
                }
            });
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
            unsafe { System.dealloc(p, layout) }
        }
        unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let _ = TRACK.try_with(|t| {
                if t.get() {
                    COUNT.with(|c| c.set(c.get() + 1));
                }
            });
            unsafe { System.realloc(p, layout, size) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: Allocator = Allocator;
    pub fn count(f: impl FnOnce()) -> usize {
        COUNT.with(|c| c.set(0));
        TRACK.with(|t| t.set(true));
        f();
        TRACK.with(|t| t.set(false));
        COUNT.with(Cell::get)
    }
}
