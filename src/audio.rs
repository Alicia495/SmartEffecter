use crate::{
    dsp::Processor,
    state::{Settings, SharedControls, Tone},
};
use cpal::{
    SampleFormat, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use rtrb::{Consumer, Producer, RingBuffer};
use serde::Serialize;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Metrics {
    pub input_peak: AtomicU32,
    pub output_peak: AtomicU32,
    pub input_clips: AtomicU64,
    pub output_clips: AtomicU64,
    pub missing_frames: AtomicU64,
    pub overflow_frames: AtomicU64,
    pub errors: AtomicU64,
    pub input_blocks: AtomicU64,
    pub output_blocks: AtomicU64,
    pub deadline_misses: AtomicU64,
    pub max_process_us: AtomicU64,
    pub input_latency_us: AtomicU64,
    pub output_latency_us: AtomicU64,
    pub queue_latency_us: AtomicU64,
    pub block_frames: AtomicU32,
    pub wave_drops: AtomicU64,
    pub mic_errors: AtomicU64,
    pub mic_discontinuities: AtomicU64,
    pub mic_error_code: AtomicU32,
    pub mic_missing_frames: AtomicU64,
    pub mic_overflow_frames: AtomicU64,
    pub mic_queue_us: AtomicU64,
}
#[derive(Serialize)]
pub struct MetricsReport {
    pub input_blocks: u64,
    pub output_blocks: u64,
    pub callback_frames: u32,
    pub input_clips: u64,
    pub output_clips: u64,
    pub missing_frames: u64,
    pub overflow_frames: u64,
    pub errors: u64,
    pub deadline_misses: u64,
    pub max_process_us: u64,
    pub driver_input_ms: f64,
    pub driver_output_ms: f64,
    pub callback_queue_ms: f64,
    pub estimated_round_trip_ms: f64,
    pub measured_round_trip_ms: Option<f64>,
    pub wave_drops: u64,
    pub mic_errors: u64,
    pub mic_discontinuities: u64,
    pub mic_error: Option<&'static str>,
    pub mic_missing_frames: u64,
    pub mic_overflow_frames: u64,
    pub mic_queue_ms: f64,
}
fn mic_error_message(code: u32) -> Option<&'static str> {
    match code {
        0 => None,
        1 => Some("マイクが切断されたか、利用できません"),
        2 => Some("マイクへのアクセスが拒否されました。Windowsのマイク権限を確認してください"),
        3 => Some("マイクの形式や接続が変更されました。再接続してください"),
        4 => Some("マイクが他のアプリで使用されています"),
        _ => Some("マイクの音声ドライバーでエラーが発生しました"),
    }
}
fn record_mic_error(metrics: &Metrics, error: &cpal::Error) {
    use cpal::ErrorKind;
    let code = match error.kind() {
        ErrorKind::Xrun => {
            metrics.mic_discontinuities.fetch_add(1, Ordering::Relaxed);
            return;
        }
        // The stream remains usable for these notifications.
        ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied => return,
        ErrorKind::DeviceNotAvailable => 1,
        ErrorKind::PermissionDenied => 2,
        ErrorKind::StreamInvalidated => 3,
        ErrorKind::DeviceBusy => 4,
        _ => 5,
    };
    metrics.mic_error_code.store(code, Ordering::Relaxed);
    metrics.errors.fetch_add(1, Ordering::Relaxed);
}
impl Metrics {
    pub fn report(&self) -> MetricsReport {
        let input = self.input_latency_us.load(Ordering::Relaxed) as f64 / 1000.;
        let output = self.output_latency_us.load(Ordering::Relaxed) as f64 / 1000.;
        let queue = self.queue_latency_us.load(Ordering::Relaxed) as f64 / 1000.;
        MetricsReport {
            input_blocks: self.input_blocks.load(Ordering::Relaxed),
            output_blocks: self.output_blocks.load(Ordering::Relaxed),
            callback_frames: self.block_frames.load(Ordering::Relaxed),
            input_clips: self.input_clips.load(Ordering::Relaxed),
            output_clips: self.output_clips.load(Ordering::Relaxed),
            missing_frames: self.missing_frames.load(Ordering::Relaxed),
            overflow_frames: self.overflow_frames.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            deadline_misses: self.deadline_misses.load(Ordering::Relaxed),
            max_process_us: self.max_process_us.load(Ordering::Relaxed),
            driver_input_ms: input,
            driver_output_ms: output,
            callback_queue_ms: queue,
            estimated_round_trip_ms: input + output + queue,
            measured_round_trip_ms: None,
            wave_drops: self.wave_drops.load(Ordering::Relaxed),
            mic_errors: self.mic_errors.load(Ordering::Relaxed),
            mic_discontinuities: self.mic_discontinuities.load(Ordering::Relaxed),
            mic_error: mic_error_message(self.mic_error_code.load(Ordering::Relaxed)),
            mic_missing_frames: self.mic_missing_frames.load(Ordering::Relaxed),
            mic_overflow_frames: self.mic_overflow_frames.load(Ordering::Relaxed),
            mic_queue_ms: self.mic_queue_us.load(Ordering::Relaxed) as f64 / 1000.,
        }
    }
}
#[derive(Clone, Copy, Default)]
pub struct WavePoint {
    pub low_in: f32,
    pub high_in: f32,
    pub low_out: f32,
    pub high_out: f32,
    pub sequence: u64,
    pub frames: u32,
}
pub struct Feed {
    pub monitor_metrics: Option<Arc<Metrics>>,
    pub wave: Consumer<WavePoint>,
    pub tuner: Consumer<f32>,
    pub metrics: Arc<Metrics>,
    pub rate: u32,
}
#[derive(Clone, Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub inputs: u16,
    pub outputs: u16,
    pub rates: Vec<u32>,
    pub buffers: Vec<u32>,
    pub input_format: String,
    pub output_format: String,
}

#[derive(Clone, Serialize)]
pub struct OutputDeviceInfo {
    pub id: String,
    pub name: String,
}
pub const VIRTUAL_OUTPUT: &str = "virtual:vb-cable";
pub const VIRTUAL_OUTPUT_HELP: &str = "https://vb-audio.com/Cable/";
const VIRTUAL_OUTPUT_MISSING: &str = "仮想出力用のVB-CABLEが見つかりません。「仮想出力の設定」から公式ドライバーを導入し、必要なら再起動後に「再検出」してください。他アプリのマイク入力にはCABLE Outputを選びます。";
fn is_vb_cable_output(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("vb-audio virtual cable") && name.contains("cable input")
}
fn find_virtual_output(outputs: &[OutputDeviceInfo]) -> Result<&OutputDeviceInfo, String> {
    let mut matches = outputs
        .iter()
        .filter(|d| d.id != VIRTUAL_OUTPUT && is_vb_cable_output(&d.name));
    let first = matches.next().ok_or(VIRTUAL_OUTPUT_MISSING)?;
    if matches.next().is_some() {
        return Err(
            "仮想出力の候補が複数あります。出力一覧から使用するCABLE Inputを指定してください。"
                .into(),
        );
    }
    Ok(first)
}
pub fn windows_outputs() -> Result<Vec<OutputDeviceInfo>, String> {
    let host = cpal::host_from_id(cpal::HostId::Wasapi).map_err(|e| e.to_string())?;
    let mut outputs = vec![OutputDeviceInfo {
        id: "windows:default".into(),
        name: "Windowsの既定の出力".into(),
    }];
    for device in host.output_devices().map_err(|e| e.to_string())? {
        if device.default_output_config().is_err() {
            continue;
        }
        if let (Ok(id), Ok(description)) = (device.id(), device.description()) {
            outputs.push(OutputDeviceInfo {
                id: id.to_string(),
                name: description.name().to_owned(),
            });
        }
    }
    let ready = find_virtual_output(&outputs).is_ok();
    outputs.insert(
        1,
        OutputDeviceInfo {
            id: VIRTUAL_OUTPUT.into(),
            name: if ready {
                "仮想出力 · VB-CABLE"
            } else {
                "仮想出力 · VB-CABLE（要セットアップ）"
            }
            .into(),
        },
    );
    Ok(outputs)
}

pub fn windows_inputs() -> Result<Vec<OutputDeviceInfo>, String> {
    let host = cpal::host_from_id(cpal::HostId::Wasapi).map_err(|e| e.to_string())?;
    let mut inputs = vec![];
    for device in host.input_devices().map_err(|e| e.to_string())? {
        if let (Ok(id), Ok(description)) = (device.id(), device.description()) {
            inputs.push(OutputDeviceInfo {
                id: id.to_string(),
                name: description.name().to_owned(),
            });
        }
    }
    Ok(inputs)
}
struct Microphone {
    stream: cpal::Stream,
    queue: Consumer<AudioFrame>,
    bridge: RateBridge,
    scratch: Vec<f32>,
    metrics: Arc<Metrics>,
}
impl Microphone {
    fn open(settings: &Settings) -> Result<Self, String> {
        let host = cpal::host_from_id(cpal::HostId::Wasapi).map_err(|e| e.to_string())?;
        let id = settings
            .mic_device
            .parse::<cpal::DeviceId>()
            .map_err(|e| e.to_string())?;
        let device = host.device_by_id(&id).ok_or("マイクが見つかりません")?;
        let config = device.default_input_config().map_err(|e| e.to_string())?;
        let channels = config.channels() as usize;
        if settings.mic_channel >= channels {
            return Err("マイクの入力チャンネルが範囲外です".into());
        }
        let channel = settings.mic_channel;
        let rate = config.sample_rate();
        let format = config.sample_format();
        let (mut tx, queue) = RingBuffer::new((rate as usize / 2).max(8192));
        let metrics = Arc::new(Metrics::default());
        let errors = metrics.clone();
        let overflows = metrics.clone();
        let mut stream_config = config.config();
        stream_config.buffer_size = cpal::BufferSize::Fixed((rate / 100).max(1));
        let stream = device
            .build_input_stream_raw(
                stream_config,
                format,
                move |data, info| {
                    overflows
                        .block_frames
                        .fetch_max((data.len() / channels) as u32, Ordering::Relaxed);
                    macro_rules! capture {
                        ($t:ty) => {
                            for frame in data.as_slice::<$t>().unwrap().chunks_exact(channels) {
                                use cpal::Sample;
                                let value = frame[channel].to_sample::<f32>();
                                let value = if value.is_finite() { value } else { 0. };
                                if tx
                                    .push(AudioFrame {
                                        samples: [value; 2],
                                        time: info.timestamp().callback,
                                    })
                                    .is_err()
                                {
                                    overflows.overflow_frames.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        };
                    }
                    match format {
                        SampleFormat::F32 => capture!(f32),
                        SampleFormat::F64 => capture!(f64),
                        SampleFormat::I16 => capture!(i16),
                        SampleFormat::I24 => capture!(cpal::I24),
                        SampleFormat::I32 => capture!(i32),
                        _ => {}
                    }
                },
                move |error| {
                    record_mic_error(&errors, &error);
                },
                None,
            )
            .map_err(|e| format!("マイク入力: {e}"))?;
        if !matches!(
            format,
            SampleFormat::F32
                | SampleFormat::F64
                | SampleFormat::I16
                | SampleFormat::I24
                | SampleFormat::I32
        ) {
            return Err(format!("未対応のマイク形式: {format}"));
        }
        Ok(Self {
            stream,
            queue,
            bridge: RateBridge::new(rate, settings.sample_rate),
            scratch: vec![0.; 16384],
            metrics,
        })
    }
}

fn host() -> Result<cpal::Host, String> {
    #[cfg(all(target_os = "windows", feature = "asio"))]
    {
        cpal::host_from_id(cpal::HostId::Asio).map_err(|e| format!("ASIOを開けません: {e}"))
    }
    #[cfg(not(all(target_os = "windows", feature = "asio")))]
    {
        Err("このビルドにはWindows ASIOが含まれていません".into())
    }
}
pub fn devices() -> Result<Vec<DeviceInfo>, String> {
    let host = host()?;
    let mut result = vec![];
    for device in host.devices().map_err(|e| e.to_string())? {
        let input = match device.default_input_config() {
            Ok(c) => c,
            Err(_) => continue,
        };
        let output = match device.default_output_config() {
            Ok(c) => c,
            Err(_) => continue,
        };
        let ranges: Vec<_> = device
            .supported_input_configs()
            .map_err(|e| e.to_string())?
            .collect();
        let out_ranges: Vec<_> = device
            .supported_output_configs()
            .map_err(|e| e.to_string())?
            .collect();
        let rates = [44100, 48000]
            .into_iter()
            .filter(|rate| {
                ranges
                    .iter()
                    .any(|r| r.min_sample_rate() <= *rate && r.max_sample_rate() >= *rate)
                    && out_ranges
                        .iter()
                        .any(|r| r.min_sample_rate() <= *rate && r.max_sample_rate() >= *rate)
            })
            .collect();
        // CPAL exposes a range, not the ASIO granularity. Every requested size is checked when opening.
        let buffers = [16, 32, 64, 128, 256, 512, 1024, 2048]
            .into_iter()
            .filter(|size| {
                [input.buffer_size(), output.buffer_size()]
                    .iter()
                    .all(|range| match range {
                        cpal::SupportedBufferSize::Range { min, max } => size >= min && size <= max,
                        _ => true,
                    })
            })
            .collect();
        result.push(DeviceInfo {
            name: device
                .description()
                .map_err(|e| e.to_string())?
                .name()
                .to_owned(),
            inputs: input.channels(),
            outputs: output.channels(),
            rates,
            buffers,
            input_format: input.sample_format().to_string(),
            output_format: output.sample_format().to_string(),
        });
    }
    Ok(result)
}

#[derive(Clone, Copy)]
struct AudioFrame {
    samples: [f32; 2],
    time: cpal::StreamInstant,
}
pub struct Session {
    monitor_stream: Option<cpal::Stream>,
    pub monitor_metrics: Option<Arc<Metrics>>,
    mic_stream: Option<cpal::Stream>,
    input: cpal::Stream,
    output: cpal::Stream,
    pub feed: Option<Feed>,
    pub metrics: Arc<Metrics>,
    pub info: String,
}
impl Session {
    pub fn open(settings: &Settings, controls: Arc<SharedControls>) -> Result<Self, String> {
        controls.instrument_gain.store(
            10f32
                .powf(settings.instrument_db.clamp(-60., 6.) / 20.)
                .to_bits(),
            Ordering::Relaxed,
        );
        controls.mic_gain.store(
            10f32.powf(settings.mic_db.clamp(-60., 6.) / 20.).to_bits(),
            Ordering::Relaxed,
        );
        let host = host()?;
        let device = host
            .devices()
            .map_err(|e| e.to_string())?
            .find(|d| {
                d.description()
                    .map(|v| v.name() == settings.device)
                    .unwrap_or(false)
            })
            .ok_or("ASIO機器が見つかりません。USB接続と公式ドライバーを確認してください。")?;
        let input = device.default_input_config().map_err(|e| e.to_string())?;
        let separate_output = !settings.output_device.is_empty();
        let monitor_enabled =
            settings.monitor_enabled && (separate_output || !settings.monitor_device.is_empty());
        let windows_monitor = if monitor_enabled && !settings.monitor_device.is_empty() {
            let host = cpal::host_from_id(cpal::HostId::Wasapi).map_err(|e| e.to_string())?;
            Some(
                if settings.monitor_device == "windows:default" {
                    host.default_output_device()
                } else {
                    let id = settings
                        .monitor_device
                        .parse::<cpal::DeviceId>()
                        .map_err(|e| e.to_string())?;
                    host.device_by_id(&id)
                }
                .ok_or(
                    "選択したモニター出力が見つかりません。自分用の出力先を選び直してください。",
                )?,
            )
        } else {
            None
        };
        controls.monitor_gain.store(
            10f32
                .powf(settings.monitor_db.clamp(-60., 6.) / 20.)
                .to_bits(),
            Ordering::Relaxed,
        );
        let windows_device = if separate_output {
            let host = cpal::host_from_id(cpal::HostId::Wasapi).map_err(|e| e.to_string())?;
            Some(
                if settings.output_device == "windows:default" {
                    host.default_output_device()
                } else if settings.output_device == VIRTUAL_OUTPUT {
                    let outputs = windows_outputs()?;
                    let endpoint = find_virtual_output(&outputs)?;
                    let id = endpoint
                        .id
                        .parse::<cpal::DeviceId>()
                        .map_err(|e| e.to_string())?;
                    host.device_by_id(&id)
                } else {
                    let id = settings
                        .output_device
                        .parse::<cpal::DeviceId>()
                        .map_err(|e| e.to_string())?;
                    host.device_by_id(&id)
                }
                .ok_or(
                    "選択したWindows出力が見つかりません。設定から出力先を選び直してください。",
                )?,
            )
        } else {
            None
        };
        let output_device = windows_device.as_ref().unwrap_or(&device);
        if let Some(monitor) = &windows_monitor {
            let monitor_id = monitor.id().map_err(|e| e.to_string())?;
            let output_id = output_device.id().map_err(|e| e.to_string())?;
            if monitor_id == output_id {
                return Err("送信用と自分用の出力先が同じです。ベースだけを聴くには別の出力先を選んでください。".into());
            }
        }
        let output = output_device
            .default_output_config()
            .map_err(|e| e.to_string())?;
        if settings.input_channel >= input.channels() as usize {
            return Err("入力チャンネルが機器の範囲外です".into());
        }
        if !separate_output && output.channels() < 2 {
            return Err("ステレオ出力が必要です".into());
        }
        let input_config = StreamConfig {
            channels: input.channels(),
            sample_rate: settings.sample_rate,
            buffer_size: cpal::BufferSize::Fixed(settings.buffer),
        };
        let output_config = StreamConfig {
            channels: output.channels(),
            sample_rate: if separate_output {
                output.sample_rate()
            } else {
                settings.sample_rate
            },
            buffer_size: if separate_output {
                // WASAPI shared mode still uses the engine period; request a
                // short ring buffer instead of its larger automatic buffer.
                cpal::BufferSize::Fixed((output.sample_rate() / 100).max(1))
            } else {
                cpal::BufferSize::Fixed(settings.buffer)
            },
        };
        let input_format = input.sample_format();
        let output_format = output.sample_format();
        for format in [input_format, output_format] {
            if !matches!(
                format,
                SampleFormat::F32
                    | SampleFormat::F64
                    | SampleFormat::I16
                    | SampleFormat::I24
                    | SampleFormat::I32
            ) {
                return Err(format!("未対応のサンプル形式です: {format}"));
            }
        }
        let (producer, consumer) = RingBuffer::new((settings.buffer as usize * 4).max(8192));
        let (wave_tx, wave_rx) = RingBuffer::new(1024);
        let (tuner_tx, tuner_rx) = RingBuffer::new(8192);
        let metrics = Arc::new(Metrics::default());
        if settings.mic_device == "asio"
            && (settings.mic_channel >= input.channels() as usize
                || settings.mic_channel == settings.input_channel)
        {
            return Err("マイクには楽器と異なる有効な入力チャンネルを選んでください".into());
        }
        let microphone = if !settings.mic_device.is_empty() && settings.mic_device != "asio" {
            Some(Microphone::open(settings)?)
        } else {
            None
        };
        let mut mic_stream = None;
        let microphone = microphone.map(|m| {
            let Microphone {
                stream,
                queue,
                bridge,
                scratch,
                metrics,
            } = m;
            mic_stream = Some(stream);
            MicFeed {
                queue,
                bridge,
                scratch,
                metrics,
            }
        });
        let monitor_capacity = if windows_monitor.is_some() {
            8192
        } else {
            (settings.buffer as usize * 4).max(64)
        };
        let (monitor_tx, monitor_rx) = RingBuffer::new(monitor_capacity);
        let monitor_metrics = monitor_enabled.then(|| Arc::new(Metrics::default()));
        let mut capture = Capture {
            monitor: monitor_enabled.then_some(monitor_tx),
            monitor_metrics: monitor_metrics.clone(),
            monitor_gain: 0.,
            microphone,
            mic_channel: (settings.mic_device == "asio").then_some(settings.mic_channel),
            instrument_gain: 10f32.powf(settings.instrument_db / 20.),
            mic_gain: 0.,
            processor: Processor::new(settings.sample_rate as f32, settings.tone),
            tone: settings.tone,
            controls,
            producer,
            wave: wave_tx,
            tuner: tuner_tx,
            metrics: metrics.clone(),
            channels: input_config.channels as usize,
            selected: settings.input_channel,
            rate: settings.sample_rate,
            point: WavePoint::default(),
            point_samples: 0,
            sample_count: 0,
        };
        let in_errors = metrics.clone();
        let input_stream = device
            .build_input_stream_raw(
                input_config,
                input_format,
                move |data, info| {
                    let start = Instant::now();
                    let stamp = info.timestamp();
                    if let Some(d) = stamp.callback.checked_duration_since(stamp.capture) {
                        capture
                            .metrics
                            .input_latency_us
                            .store(d.as_micros() as u64, Ordering::Relaxed);
                    }
                    capture.controls.snapshot(&mut capture.tone);
                    capture.processor.set_tone(capture.tone);
                    match input_format {
                        SampleFormat::F32 => {
                            capture.run(data.as_slice::<f32>().unwrap(), stamp.callback)
                        }
                        SampleFormat::F64 => {
                            capture.run(data.as_slice::<f64>().unwrap(), stamp.callback)
                        }
                        SampleFormat::I16 => {
                            capture.run(data.as_slice::<i16>().unwrap(), stamp.callback)
                        }
                        SampleFormat::I24 => {
                            capture.run(data.as_slice::<cpal::I24>().unwrap(), stamp.callback)
                        }
                        SampleFormat::I32 => {
                            capture.run(data.as_slice::<i32>().unwrap(), stamp.callback)
                        }
                        _ => {
                            capture.metrics.errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    let elapsed = start.elapsed().as_micros() as u64;
                    capture
                        .metrics
                        .max_process_us
                        .fetch_max(elapsed, Ordering::Relaxed);
                    let frames = data.len() / capture.channels;
                    capture
                        .metrics
                        .block_frames
                        .store(frames as u32, Ordering::Relaxed);
                    if elapsed > frames as u64 * 1_000_000 / capture.rate as u64 {
                        capture
                            .metrics
                            .deadline_misses
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    capture.metrics.input_blocks.fetch_add(1, Ordering::Relaxed);
                },
                move |_| {
                    in_errors.errors.fetch_add(1, Ordering::Relaxed);
                },
                None,
            )
            .map_err(|e| format!("ASIO入力を開始できません: {e}"))?;
        let playback = Playback {
            consumer,
            metrics: metrics.clone(),
            channels: output_config.channels as usize,
            bridge: separate_output
                .then(|| RateBridge::new(settings.sample_rate, output_config.sample_rate)),
        };
        let output_stream =
            build_playback_stream(output_device, output_config, output_format, playback)?;
        let monitor_stream = if let Some(monitor_metrics) = &monitor_metrics {
            let monitor_device = windows_monitor.as_ref().unwrap_or(&device);
            let config = monitor_device
                .default_output_config()
                .map_err(|e| e.to_string())?;
            let separate_monitor = windows_monitor.is_some();
            if !separate_monitor && config.channels() < 2 {
                return Err("ベースモニターにはASIOステレオ出力が必要です".into());
            }
            let playback = Playback {
                consumer: monitor_rx,
                metrics: monitor_metrics.clone(),
                channels: config.channels() as usize,
                bridge: separate_monitor
                    .then(|| RateBridge::new(settings.sample_rate, config.sample_rate())),
            };
            Some(
                build_playback_stream(
                    monitor_device,
                    StreamConfig {
                        channels: config.channels(),
                        sample_rate: if separate_monitor {
                            config.sample_rate()
                        } else {
                            settings.sample_rate
                        },
                        buffer_size: cpal::BufferSize::Fixed(if separate_monitor {
                            (config.sample_rate() / 100).max(1)
                        } else {
                            settings.buffer
                        }),
                    },
                    config.sample_format(),
                    playback,
                )
                .map_err(|e| format!("ベースモニター: {e}"))?,
            )
        } else {
            None
        };
        if let Some(stream) = &mic_stream {
            stream.play().map_err(|e| e.to_string())?;
        }
        input_stream.play().map_err(|e| e.to_string())?;
        output_stream.play().map_err(|e| e.to_string())?;
        if let Some(stream) = &monitor_stream {
            stream.play().map_err(|e| e.to_string())?;
        }
        let info = if separate_output {
            format!(
                "{} → {} · Windows出力",
                settings.device,
                output_device
                    .description()
                    .map_err(|e| e.to_string())?
                    .name()
            )
        } else {
            format!(
                "{} · {} Hz · {} samples",
                settings.device, settings.sample_rate, settings.buffer
            )
        };
        Ok(Self {
            monitor_stream,
            monitor_metrics: monitor_metrics.clone(),
            mic_stream,
            input: input_stream,
            output: output_stream,
            feed: Some(Feed {
                monitor_metrics,
                wave: wave_rx,
                tuner: tuner_rx,
                metrics: metrics.clone(),
                rate: settings.sample_rate,
            }),
            metrics,
            info,
        })
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Some(stream) = &self.monitor_stream {
            let _ = stream.pause();
        }
        let _ = self.output.pause();
        let _ = self.input.pause();
        if let Some(stream) = &self.mic_stream {
            let _ = stream.pause();
        }
    }
}
fn build_playback_stream(
    device: &cpal::Device,
    config: StreamConfig,
    format: SampleFormat,
    mut playback: Playback,
) -> Result<cpal::Stream, String> {
    if !matches!(
        format,
        SampleFormat::F32
            | SampleFormat::F64
            | SampleFormat::I16
            | SampleFormat::I24
            | SampleFormat::I32
    ) {
        return Err(format!("未対応の出力形式: {format}"));
    }
    let out_errors = playback.metrics.clone();
    let output_stream = device
        .build_output_stream_raw(
            config,
            format,
            move |data, info| {
                let stamp = info.timestamp();
                if let Some(d) = stamp.playback.checked_duration_since(stamp.callback) {
                    playback
                        .metrics
                        .output_latency_us
                        .store(d.as_micros() as u64, Ordering::Relaxed);
                }
                match format {
                    SampleFormat::F32 => {
                        playback.run(data.as_slice_mut::<f32>().unwrap(), stamp.callback)
                    }
                    SampleFormat::F64 => {
                        playback.run(data.as_slice_mut::<f64>().unwrap(), stamp.callback)
                    }
                    SampleFormat::I16 => {
                        playback.run(data.as_slice_mut::<i16>().unwrap(), stamp.callback)
                    }
                    SampleFormat::I24 => {
                        playback.run(data.as_slice_mut::<cpal::I24>().unwrap(), stamp.callback)
                    }
                    SampleFormat::I32 => {
                        playback.run(data.as_slice_mut::<i32>().unwrap(), stamp.callback)
                    }
                    _ => {
                        playback.metrics.errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                playback
                    .metrics
                    .output_blocks
                    .fetch_add(1, Ordering::Relaxed);
            },
            move |_| {
                out_errors.errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| format!("出力を開始できません: {e}"))?;
    Ok(output_stream)
}

struct MicFeed {
    queue: Consumer<AudioFrame>,
    bridge: RateBridge,
    scratch: Vec<f32>,
    metrics: Arc<Metrics>,
}
struct Capture {
    monitor: Option<Producer<AudioFrame>>,
    monitor_metrics: Option<Arc<Metrics>>,
    monitor_gain: f32,
    microphone: Option<MicFeed>,
    mic_channel: Option<usize>,
    instrument_gain: f32,
    mic_gain: f32,
    processor: Processor,
    tone: Tone,
    controls: Arc<SharedControls>,
    producer: Producer<AudioFrame>,
    wave: Producer<WavePoint>,
    tuner: Producer<f32>,
    metrics: Arc<Metrics>,
    channels: usize,
    selected: usize,
    rate: u32,
    point: WavePoint,
    point_samples: u32,
    sample_count: u64,
}
impl Capture {
    fn run<T: cpal::SizedSample>(&mut self, data: &[T], time: cpal::StreamInstant)
    where
        f32: cpal::FromSample<T>,
    {
        let bypass = self.controls.bypass.load(Ordering::Relaxed) != 0;
        let mute = self.controls.mute.load(Ordering::Relaxed) != 0;
        let mut input_peak = 0f32;
        let mut output_peak = 0f32;
        if let Some(mic) = &mut self.microphone {
            // Capture callbacks may arrive in larger bursts than the ASIO callbacks.
            // Retain one source period plus margin; never delay the instrument path.
            mic.bridge.minimum_frames = mic.metrics.block_frames.load(Ordering::Relaxed) as usize
                + (mic.bridge.input_rate * 0.003) as usize;
            let count = (data.len() / self.channels).min(mic.scratch.len());
            mic.bridge
                .run(&mut mic.scratch[..count], 1, &mut mic.queue, &mic.metrics);
        }
        if let Some(mic) = &self.microphone {
            self.metrics.mic_discontinuities.store(
                mic.metrics.mic_discontinuities.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            self.metrics.mic_error_code.store(
                mic.metrics.mic_error_code.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            self.metrics.mic_errors.store(
                mic.metrics.errors.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            self.metrics.mic_missing_frames.store(
                mic.metrics.missing_frames.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            self.metrics.mic_overflow_frames.store(
                mic.metrics.overflow_frames.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            self.metrics.mic_queue_us.store(
                mic.metrics.queue_latency_us.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
        }
        let instrument_target =
            f32::from_bits(self.controls.instrument_gain.load(Ordering::Relaxed));
        let mic_target = if mute {
            0.
        } else {
            f32::from_bits(self.controls.mic_gain.load(Ordering::Relaxed))
        };
        let monitor_target = f32::from_bits(self.controls.monitor_gain.load(Ordering::Relaxed));
        for (index, frame) in data.chunks_exact(self.channels).enumerate() {
            let raw = frame[self.selected].to_sample::<f32>();
            let (input, mut out) = self.processor.process(raw, bypass, mute);
            self.instrument_gain += (instrument_target - self.instrument_gain) * 0.004;
            self.mic_gain += (mic_target - self.mic_gain) * 0.004;
            self.monitor_gain += (monitor_target - self.monitor_gain) * 0.004;
            if let Some(monitor) = &mut self.monitor {
                let samples =
                    out.map(|s| (s * self.instrument_gain * self.monitor_gain).clamp(-1., 1.));
                if monitor.push(AudioFrame { samples, time }).is_err()
                    && let Some(metrics) = &self.monitor_metrics
                {
                    metrics.overflow_frames.fetch_add(1, Ordering::Relaxed);
                }
            }
            let mic = self
                .mic_channel
                .map(|c| frame[c].to_sample::<f32>())
                .or_else(|| {
                    self.microphone
                        .as_ref()
                        .and_then(|m| m.scratch.get(index).copied())
                })
                .unwrap_or(0.);
            let mic = if mic.is_finite() { mic } else { 0. };
            for sample in &mut out {
                *sample = *sample * self.instrument_gain + mic * self.mic_gain;
            }
            input_peak = input_peak.max(input.abs()).max(mic.abs());
            output_peak = output_peak.max(out[0].abs()).max(out[1].abs());
            if raw.abs() >= 0.999 || input.abs() >= 1. || mic.abs() >= 0.999 {
                self.metrics.input_clips.fetch_add(1, Ordering::Relaxed);
            }
            if out.iter().any(|v| v.abs() > 1.) {
                self.metrics.output_clips.fetch_add(1, Ordering::Relaxed);
            }
            for sample in &mut out {
                *sample = sample.clamp(-1., 1.);
            }
            if self
                .producer
                .push(AudioFrame { samples: out, time })
                .is_err()
            {
                self.metrics.overflow_frames.fetch_add(1, Ordering::Relaxed);
            }
            self.point.low_in = self.point.low_in.min(input);
            self.point.high_in = self.point.high_in.max(input);
            self.point.low_out = self.point.low_out.min(out[0]).min(out[1]);
            self.point.high_out = self.point.high_out.max(out[0]).max(out[1]);
            self.point_samples += 1;
            self.sample_count += 1;
            if self.sample_count.is_multiple_of(4) {
                let _ = self.tuner.push(input);
            }
            if self.point_samples >= self.rate / 200 {
                self.point.sequence = self.sample_count;
                self.point.frames = self.point_samples;
                if self.wave.push(self.point).is_err() {
                    self.metrics.wave_drops.fetch_add(1, Ordering::Relaxed);
                }
                self.point = WavePoint::default();
                self.point_samples = 0;
            }
        }
        self.metrics
            .input_peak
            .fetch_max(input_peak.to_bits(), Ordering::Relaxed);
        self.metrics
            .output_peak
            .fetch_max(output_peak.to_bits(), Ordering::Relaxed);
    }
}
struct Playback {
    consumer: Consumer<AudioFrame>,
    metrics: Arc<Metrics>,
    channels: usize,
    bridge: Option<RateBridge>,
}
impl Playback {
    fn run<T: cpal::SizedSample + cpal::FromSample<f32>>(
        &mut self,
        data: &mut [T],
        time: cpal::StreamInstant,
    ) {
        if let Some(bridge) = &mut self.bridge {
            bridge.run(data, self.channels, &mut self.consumer, &self.metrics);
            return;
        }
        for (index, frame) in data.chunks_exact_mut(self.channels).enumerate() {
            frame.fill(T::EQUILIBRIUM);
            match self.consumer.pop() {
                Ok(captured) => {
                    frame[0] = T::from_sample(captured.samples[0]);
                    frame[1] = T::from_sample(captured.samples[1]);
                    if index == 0
                        && let Some(d) = time.checked_duration_since(captured.time)
                    {
                        self.metrics
                            .queue_latency_us
                            .store(d.as_micros() as u64, Ordering::Relaxed);
                    }
                }
                Err(_) => {
                    if self.metrics.output_blocks.load(Ordering::Relaxed) > 4 {
                        self.metrics.missing_frames.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
    }
}

// Separate hardware clocks need a small elastic queue and fractional reads.
// This is never used by the same-device ASIO path.
struct RateBridge {
    input_rate: f64,
    ratio: f64,
    base_ratio: f64,
    target: usize,
    minimum_frames: usize,
    phase: f64,
    previous: [f32; 2],
    next: [f32; 2],
    started: bool,
    fade: f32,
    output_rate: f32,
}
impl RateBridge {
    fn new(input_rate: u32, output_rate: u32) -> Self {
        let ratio = input_rate as f64 / output_rate as f64;
        Self {
            input_rate: input_rate as f64,
            ratio,
            base_ratio: ratio,
            target: 0,
            minimum_frames: 0,
            phase: 0.,
            previous: [0.; 2],
            next: [0.; 2],
            started: false,
            fade: 0.,
            output_rate: output_rate as f32,
        }
    }
    fn run<T: cpal::SizedSample + cpal::FromSample<f32>>(
        &mut self,
        data: &mut [T],
        channels: usize,
        queue: &mut Consumer<AudioFrame>,
        metrics: &Metrics,
    ) {
        data.fill(T::EQUILIBRIUM);
        if channels == 0 {
            return;
        }
        let frames = data.len() / channels;
        if !self.started {
            // The first WASAPI call can request the entire endpoint buffer.
            // Recalculate while priming so it cannot impose that larger queue
            // on all subsequent (usually smaller) callbacks.
            self.target = ((self.input_rate * 0.01) as usize)
                .max(self.minimum_frames)
                .max(
                    (frames as f64 * self.base_ratio).ceil() as usize
                        + (self.input_rate * 0.003) as usize,
                )
                .min(queue.buffer().capacity() / 2);
        }
        let mut available = queue.slots();
        metrics.queue_latency_us.store(
            (available as f64 / self.input_rate * 1_000_000.) as u64,
            Ordering::Relaxed,
        );
        if !self.started {
            if available < self.target.max(2) {
                return;
            }
            for _ in 0..available.saturating_sub(self.target) {
                let _ = queue.pop();
            }
            available = self.target;
            self.previous = queue.pop().unwrap().samples;
            self.next = queue.pop().unwrap().samples;
            self.phase = 0.;
            self.fade = 0.;
            self.started = true;
        }
        // Smooth a bounded correction to absorb independent device clock drift.
        let error = (available as f64 - self.target as f64) / self.target as f64;
        let wanted = self.base_ratio * (1. + (error * 0.003).clamp(-0.005, 0.005));
        self.ratio += (wanted - self.ratio) * 0.05;
        // Recover from a suspended output device without playing stale audio.
        if available > self.target * 3 {
            let discard = available - self.target;
            for _ in 0..discard {
                let _ = queue.pop();
            }
            metrics
                .overflow_frames
                .fetch_add(discard as u64, Ordering::Relaxed);
            self.started = false;
            return;
        }
        for (index, frame) in data.chunks_exact_mut(channels).enumerate() {
            while self.phase >= 1. {
                self.previous = self.next;
                match queue.pop() {
                    Ok(value) => self.next = value.samples,
                    Err(_) => {
                        metrics
                            .missing_frames
                            .fetch_add((frames - index) as u64, Ordering::Relaxed);
                        self.started = false;
                        return;
                    }
                }
                self.phase -= 1.;
            }
            self.fade = (self.fade + 1. / (self.output_rate * 0.005)).min(1.);
            let value: [f32; 2] = std::array::from_fn(|i| {
                (self.previous[i] + (self.next[i] - self.previous[i]) * self.phase as f32)
                    * self.fade
            });
            if channels == 1 {
                frame[0] = T::from_sample((value[0] + value[1]) * 0.5);
            } else {
                frame[0] = T::from_sample(value[0]);
                frame[1] = T::from_sample(value[1]);
            }
            self.phase += self.ratio;
        }
    }
}

pub enum Command {
    Scan,
    Start(Box<Settings>),
    Stop,
    Shutdown,
}
pub enum Event {
    Devices(Result<Vec<DeviceInfo>, String>),
    Outputs(Result<Vec<OutputDeviceInfo>, String>),
    Inputs(Result<Vec<OutputDeviceInfo>, String>),
    Started(Result<(Feed, String), String>),
    Stopped,
}
pub fn spawn_worker(
    controls: Arc<SharedControls>,
) -> (
    std::sync::mpsc::Sender<Command>,
    std::sync::mpsc::Receiver<Event>,
    std::thread::JoinHandle<()>,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let (events, out) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut session: Option<Session> = None;
        while let Ok(command) = rx.recv() {
            match command {
                Command::Scan => {
                    if session.is_none() {
                        let _ = events.send(Event::Devices(devices()));
                        let _ = events.send(Event::Outputs(windows_outputs()));
                        let _ = events.send(Event::Inputs(windows_inputs()));
                    }
                }
                Command::Start(settings) => {
                    drop(session.take());
                    let opened = Session::open(&settings, controls.clone());
                    match opened {
                        Ok(mut next) => {
                            let feed = next.feed.take().unwrap();
                            let info = next.info.clone();
                            session = Some(next);
                            let _ = events.send(Event::Started(Ok((feed, info))));
                        }
                        Err(error) => {
                            let _ = events.send(Event::Started(Err(error)));
                        }
                    }
                }
                Command::Stop => {
                    controls.mute.store(1, Ordering::Relaxed);
                    std::thread::sleep(Duration::from_millis(10));
                    drop(session.take());
                    let _ = events.send(Event::Stopped);
                }
                Command::Shutdown => {
                    controls.mute.store(1, Ordering::Relaxed);
                    std::thread::sleep(Duration::from_millis(10));
                    break;
                }
            }
        }
    });
    (tx, out, worker)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn microphone_discontinuity_does_not_mark_a_live_stream_as_failed() {
        let metrics = Metrics::default();
        let allocations = crate::allocation_check::count(|| {
            record_mic_error(&metrics, &cpal::ErrorKind::Xrun.into());
            record_mic_error(&metrics, &cpal::ErrorKind::DeviceChanged.into());
            record_mic_error(&metrics, &cpal::ErrorKind::RealtimeDenied.into());
        });
        assert_eq!(allocations, 0);
        assert_eq!(metrics.mic_discontinuities.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.errors.load(Ordering::Relaxed), 0);
        record_mic_error(&metrics, &cpal::ErrorKind::DeviceNotAvailable.into());
        assert_eq!(metrics.errors.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.mic_error_code.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn virtual_output_never_falls_back_to_physical_or_default_output() {
        let mut outputs = vec![
            OutputDeviceInfo {
                id: "windows:default".into(),
                name: "Windowsの既定の出力".into(),
            },
            OutputDeviceInfo {
                id: "speaker".into(),
                name: "Speakers (Realtek)".into(),
            },
            OutputDeviceInfo {
                id: VIRTUAL_OUTPUT.into(),
                name: "仮想出力 · VB-CABLE".into(),
            },
            OutputDeviceInfo {
                id: "capture".into(),
                name: "CABLE Output (VB-Audio Virtual Cable)".into(),
            },
        ];
        assert!(find_virtual_output(&outputs).is_err());
        outputs.push(OutputDeviceInfo {
            id: "16ch".into(),
            name: "CABLE In 16ch (VB-Audio Virtual Cable)".into(),
        });
        outputs.push(OutputDeviceInfo {
            id: "cable".into(),
            name: "CABLE Input (VB-Audio Virtual Cable)".into(),
        });
        assert_eq!(find_virtual_output(&outputs).unwrap().id, "cable");
        outputs.push(OutputDeviceInfo {
            id: "other".into(),
            name: "CABLE Input (VB-Audio Virtual Cable)".into(),
        });
        assert!(
            find_virtual_output(&outputs)
                .err()
                .unwrap()
                .contains("複数")
        );
    }
    #[test]
    fn microphone_bursts_do_not_starve_small_asio_blocks() {
        let (mut tx, mut rx) = RingBuffer::new(8192);
        let metrics = Metrics::default();
        let mut bridge = RateBridge::new(48000, 48000);
        bridge.minimum_frames = 960 + 144; // 20 ms capture period plus 3 ms margin.
        let mut out = [0f32; 32];
        let allocations = crate::allocation_check::count(|| {
            for block in 0..15000 {
                if block % 30 == 0 {
                    for _ in 0..960 {
                        tx.push(AudioFrame {
                            samples: [0.2; 2],
                            time: cpal::StreamInstant::ZERO,
                        })
                        .unwrap();
                    }
                }
                bridge.run(&mut out, 1, &mut rx, &metrics);
                if block > 60 {
                    assert!(out.iter().all(|v| (*v - 0.2).abs() < 0.0001));
                }
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(metrics.missing_frames.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.overflow_frames.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn windows_bridge_resamples_and_tracks_independent_clocks_without_allocations() {
        for (input_rate, output_rate, skew) in [
            (48000, 44100, 1.0005),
            (44100, 48000, 0.9995),
            (48000, 48000, 1.001),
        ] {
            let (mut producer, mut consumer) = RingBuffer::new(8192);
            let metrics = Metrics::default();
            let mut bridge = RateBridge::new(input_rate, output_rate);
            let frames = output_rate as usize / 100;
            let mut output = vec![0f32; frames * 2];
            let mut position = 0usize;
            let mut remainder = 0f64;
            let mut crossings = 0usize;
            let mut previous = 0f32;
            let allocations = crate::allocation_check::count(|| {
                for block in 0..1000 {
                    remainder += input_rate as f64 / 100. * skew;
                    let count = remainder as usize;
                    remainder -= count as f64;
                    for _ in 0..count {
                        let signal = (std::f32::consts::TAU * 1000. * position as f32
                            / input_rate as f32)
                            .sin()
                            * 0.2;
                        producer
                            .push(AudioFrame {
                                samples: [signal, signal],
                                time: cpal::StreamInstant::ZERO,
                            })
                            .unwrap();
                        position += 1;
                    }
                    bridge.run(&mut output, 2, &mut consumer, &metrics);
                    if block >= 100 {
                        for frame in output.as_chunks::<2>().0 {
                            assert!(frame[0].is_finite() && frame[0].abs() <= 0.201);
                            if previous <= 0. && frame[0] > 0. {
                                crossings += 1;
                            }
                            previous = frame[0];
                        }
                    }
                }
            });
            assert_eq!(allocations, 0);
            assert_eq!(metrics.missing_frames.load(Ordering::Relaxed), 0);
            assert_eq!(metrics.overflow_frames.load(Ordering::Relaxed), 0);
            assert!(
                (8950..9050).contains(&crossings),
                "frequency crossings: {crossings}"
            );
            assert!(consumer.slots() < 2048);
        }
    }
    #[test]
    fn windows_bridge_downmixes_mono_and_recovers_from_starvation() {
        let (mut producer, mut consumer) = RingBuffer::new(4096);
        let mut bridge = RateBridge::new(48000, 48000);
        let metrics = Metrics::default();
        let mut output = [0f32; 480];
        for _ in 0..1440 {
            producer
                .push(AudioFrame {
                    samples: [0.2, 0.4],
                    time: cpal::StreamInstant::ZERO,
                })
                .unwrap();
        }
        bridge.run(&mut output, 1, &mut consumer, &metrics);
        assert!((output[479] - 0.3).abs() < 0.0001);
        for _ in 0..5 {
            bridge.run(&mut output, 1, &mut consumer, &metrics);
        }
        assert!(!bridge.started);
        assert!(metrics.missing_frames.load(Ordering::Relaxed) > 0);
        assert!(output.iter().all(|v| *v == 0.));
        for _ in 0..1440 {
            producer
                .push(AudioFrame {
                    samples: [0.2, 0.4],
                    time: cpal::StreamInstant::ZERO,
                })
                .unwrap();
        }
        bridge.run(&mut output, 1, &mut consumer, &metrics);
        assert!(bridge.started);
        assert!((output[479] - 0.3).abs() < 0.0001);
    }
    #[test]
    fn callback_path_is_allocation_free_and_routes_input_two() {
        let mut tone = Tone::default();
        for effect in &mut tone.effects {
            effect.enabled = false;
        }
        tone.output_db = 0.;
        let controls = Arc::new(SharedControls::new(&tone));
        controls
            .monitor_gain
            .store(1f32.to_bits(), Ordering::Relaxed);
        let metrics = Arc::new(Metrics::default());
        let (producer, consumer) = RingBuffer::new(256);
        let (wave, _wave_rx) = RingBuffer::new(100);
        let (tuner, _tuner_rx) = RingBuffer::new(4096);
        let (monitor_tx, mut monitor_rx) = RingBuffer::new(256);
        let mut capture = Capture {
            monitor: Some(monitor_tx),
            monitor_metrics: None,
            monitor_gain: 1.,
            microphone: None,
            mic_channel: None,
            instrument_gain: 1.,
            mic_gain: 0.,
            processor: Processor::new(48000., tone),
            tone,
            controls,
            producer,
            wave,
            tuner,
            metrics: metrics.clone(),
            channels: 2,
            selected: 1,
            rate: 48000,
            point: WavePoint::default(),
            point_samples: 0,
            sample_count: 0,
        };
        let mut playback = Playback {
            consumer,
            metrics,
            channels: 2,
            bridge: None,
        };
        let input: [f32; 128] = std::array::from_fn(|i| if i % 2 == 0 { 0.9 } else { 0.2 });
        let mut output = [0f32; 128];
        let allocations = crate::allocation_check::count(|| {
            for _ in 0..100 {
                capture.run(&input, cpal::StreamInstant::ZERO);
                playback.run(&mut output, cpal::StreamInstant::ZERO);
                while let Ok(frame) = monitor_rx.pop() {
                    assert!(frame.samples.iter().all(|s| s.abs() <= 0.201));
                }
            }
        });
        assert_eq!(allocations, 0);
        assert!(output.iter().all(|v| (*v - 0.2).abs() < 1e-5));
        capture.mic_channel = Some(0);
        capture
            .controls
            .instrument_gain
            .store(0.5f32.to_bits(), Ordering::Relaxed);
        capture
            .controls
            .mic_gain
            .store(0.25f32.to_bits(), Ordering::Relaxed);
        let allocations = crate::allocation_check::count(|| {
            for _ in 0..100 {
                capture.run(&input, cpal::StreamInstant::ZERO);
                playback.run(&mut output, cpal::StreamInstant::ZERO);
                while let Ok(frame) = monitor_rx.pop() {
                    assert!(frame.samples.iter().all(|s| s.abs() <= 0.201));
                }
            }
        });
        assert_eq!(allocations, 0);
        assert!(output.iter().all(|v| (*v - 0.325).abs() < 1e-5));
        capture.run(&input, cpal::StreamInstant::ZERO);
        playback.run(&mut output, cpal::StreamInstant::ZERO);
        while let Ok(frame) = monitor_rx.pop() {
            assert!(frame.samples.iter().all(|v| (*v - 0.1).abs() < 1e-5));
        }

        capture.controls.mute.store(1, Ordering::Relaxed);
        capture.run(&input, cpal::StreamInstant::ZERO);
        playback.run(&mut output, cpal::StreamInstant::ZERO);
        assert!(output[0] > 0.32, "mute must fade rather than jump");
        assert!(
            output
                .windows(2)
                .all(|pair| (pair[0] - pair[1]).abs() < 0.005)
        );
        capture.controls.mute.store(0, Ordering::Relaxed);
        for _ in 0..100 {
            capture.run(&input, cpal::StreamInstant::ZERO);
            playback.run(&mut output, cpal::StreamInstant::ZERO);
        }
        // Exercise the separate USB microphone bridge in the actual mixer callback.
        capture.mic_channel = None;
        let (mut mic_tx, mic_rx) = RingBuffer::new(8192);
        for _ in 0..600 {
            mic_tx
                .push(AudioFrame {
                    samples: [0.4; 2],
                    time: cpal::StreamInstant::ZERO,
                })
                .unwrap();
        }
        capture.microphone = Some(MicFeed {
            queue: mic_rx,
            bridge: RateBridge::new(44100, 48000),
            scratch: vec![0.; 64],
            metrics: Arc::new(Metrics::default()),
        });
        let allocations = crate::allocation_check::count(|| {
            for block in 0..100 {
                for _ in 0..if block % 5 == 0 { 58 } else { 59 } {
                    mic_tx
                        .push(AudioFrame {
                            samples: [0.4; 2],
                            time: cpal::StreamInstant::ZERO,
                        })
                        .unwrap();
                }
                capture.run(&input, cpal::StreamInstant::ZERO);
                playback.run(&mut output, cpal::StreamInstant::ZERO);
                while let Ok(frame) = monitor_rx.pop() {
                    assert!(frame.samples.iter().all(|s| s.abs() <= 0.201));
                }
            }
        });
        assert_eq!(allocations, 0);
        assert!(output.iter().all(|v| (*v - 0.2).abs() < 0.001));
        capture.controls.mute.store(1, Ordering::Relaxed);
        for _ in 0..100 {
            capture.run(&input, cpal::StreamInstant::ZERO);
            playback.run(&mut output, cpal::StreamInstant::ZERO);
        }
        assert!(output.iter().all(|v| v.abs() < 1e-5));
    }
}
