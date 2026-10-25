use crate::{
    AppWindow, EffectRow, ParameterRow,
    audio::{self, Command, DeviceInfo, Event, Feed, WavePoint},
    dsp,
    state::{self, Preset, Settings, SharedControls, Tone},
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::Rc,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

struct AppState {
    settings: Settings,
    controls: Arc<SharedControls>,
    devices: Vec<DeviceInfo>,
    outputs: Vec<audio::OutputDeviceInfo>,
    monitor_outputs: Vec<audio::OutputDeviceInfo>,
    inputs: Vec<audio::OutputDeviceInfo>,
    feed: Option<Feed>,
    wave: VecDeque<WavePoint>,
    tuner: VecDeque<f32>,
    parameters: Rc<VecModel<ParameterRow>>,
    last_edit: Option<Instant>,
    last_pitch: Instant,
    last_clip: Instant,
    last_blocks: u64,
    last_audio: Instant,
    demo_processor: Option<dsp::Processor>,
    demo_sample: u64,
}
fn strings(values: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    Rc::new(VecModel::from(
        values
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
    .into()
}
fn display(v: f32, unit: &str) -> String {
    if unit == "Hz" || v.fract().abs() > 0.01 {
        format!("{v:.1} {unit}")
    } else {
        format!("{v:.0} {unit}")
    }
}
fn refresh_effect(ui: &AppWindow, state: &mut AppState) {
    let index = ui.get_selected().clamp(0, 6) as usize;
    ui.set_effect_name(state::EFFECT_NAMES[index].into());
    ui.set_effect_hint(state::EFFECT_HINTS[index].into());
    ui.set_effect_number(format!("{:02} / 07", index + 1).into());
    ui.set_effects(
        Rc::new(VecModel::from(
            (0..7)
                .map(|i| EffectRow {
                    name: state::EFFECT_NAMES[i].into(),
                    enabled: state.settings.tone.effects[i].enabled,
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    state.parameters = Rc::new(VecModel::from(
        state::specs(index)
            .iter()
            .enumerate()
            .filter(|(j, _)| index != 3 || ui.get_eq_open() || *j == 0 || *j == 4)
            .map(|(j, p)| {
                let value = state.settings.tone.effects[index].params[j];
                ParameterRow {
                    index: j as i32,
                    name: p.name.into(),
                    value,
                    minimum: p.min,
                    maximum: p.max,
                    step: p.step,
                    default_value: p.default,
                    display: display(value, p.unit).into(),
                }
            })
            .collect::<Vec<_>>(),
    ));
    ui.set_parameters(state.parameters.clone().into());
    ui.set_mode_names(strings(match index {
        2 => vec!["Overdrive".into(), "Distortion".into()],
        3 => vec!["Clean".into(), "Crunch".into()],
        _ => vec![],
    }));
    ui.set_mode_index(state.settings.tone.effects[index].mode as i32);
    ui.set_input_db(state.settings.tone.input_db);
    ui.set_output_db(state.settings.tone.output_db);
    ui.set_bass_mode(state.settings.tone.bass);
    ui.set_instrument_db(state.settings.instrument_db);
    ui.set_mic_db(state.settings.mic_db);
    ui.set_monitor_enabled(state.settings.monitor_enabled);
    ui.set_monitor_db(state.settings.monitor_db);
}
fn refresh_presets(ui: &AppWindow, state: &AppState) {
    ui.set_preset_names(strings(
        ["現在の音色", "Clean", "Crunch", "Lead"]
            .map(str::to_owned)
            .into_iter()
            .chain(state.settings.presets.iter().map(|p| p.name.clone())),
    ));
    ui.set_user_preset(ui.get_preset_index() >= 4);
}
fn changed(ui: &AppWindow, state: &mut AppState) {
    state.controls.instrument_gain.store(
        10f32.powf(state.settings.instrument_db / 20.).to_bits(),
        Ordering::Relaxed,
    );
    state.controls.mic_gain.store(
        10f32.powf(state.settings.mic_db / 20.).to_bits(),
        Ordering::Relaxed,
    );
    state.controls.monitor_gain.store(
        10f32.powf(state.settings.monitor_db / 20.).to_bits(),
        Ordering::Relaxed,
    );
    state.controls.publish(&state.settings.tone);
    state.last_edit = Some(Instant::now());
    ui.set_dirty(true);
}
fn edit_parameter(ui: &AppWindow, state: &mut AppState, index: i32, value: f32) {
    let effect = ui.get_selected() as usize;
    if !value.is_finite() {
        return;
    }
    if let Some(spec) = state::specs(effect).get(index as usize) {
        let value = ((value / spec.step).round() * spec.step).clamp(spec.min, spec.max);
        state.settings.tone.effects[effect].params[index as usize] = value;
        for row_index in 0..state.parameters.row_count() {
            if let Some(mut row) = state.parameters.row_data(row_index)
                && row.index == index
            {
                row.value = value;
                row.display = display(value, spec.unit).into();
                state.parameters.set_row_data(row_index, row);
                break;
            }
        }
        changed(ui, state);
    }
}
fn device_options(ui: &AppWindow, state: &AppState, index: usize) {
    if let Some(device) = state.devices.get(index) {
        ui.set_device_label(device.name.clone().into());
        ui.set_channel_names(strings(
            (0..device.inputs).map(|i| format!("Input {}", i + 1)),
        ));
        ui.set_channel_index(
            state
                .settings
                .input_channel
                .min(device.inputs.saturating_sub(1) as usize) as i32,
        );
        ui.set_rate_names(strings(device.rates.iter().map(|v| format!("{v} Hz"))));
        ui.set_rate_index(
            device
                .rates
                .iter()
                .position(|v| *v == state.settings.sample_rate)
                .unwrap_or(0) as i32,
        );
        ui.set_buffer_names(strings(
            device.buffers.iter().map(|v| format!("{v} samples")),
        ));
        ui.set_buffer_index(
            device
                .buffers
                .iter()
                .position(|v| *v == state.settings.buffer)
                .unwrap_or(0) as i32,
        );
    } else {
        ui.set_channel_names(strings([]));
        ui.set_rate_names(strings([]));
        ui.set_buffer_names(strings([]));
    }
}
fn refresh_monitor_outputs(
    ui: &AppWindow,
    state: &mut AppState,
    outputs: &[audio::OutputDeviceInfo],
) {
    let mut monitors: Vec<_> = outputs
        .iter()
        .filter(|d| d.id != audio::VIRTUAL_OUTPUT)
        .cloned()
        .collect();
    if !state.settings.monitor_device.is_empty()
        && !monitors
            .iter()
            .any(|d| d.id == state.settings.monitor_device)
    {
        monitors.push(audio::OutputDeviceInfo {
            id: state.settings.monitor_device.clone(),
            name: "未接続のモニター（選び直してください）".into(),
        });
    }
    ui.set_monitor_names(strings(
        std::iter::once("入力機器 · ASIO（低遅延）".into())
            .chain(monitors.iter().map(|d| d.name.clone())),
    ));
    ui.set_monitor_index(
        monitors
            .iter()
            .position(|d| d.id == state.settings.monitor_device)
            .map(|i| i + 1)
            .unwrap_or(0) as i32,
    );
    state.monitor_outputs = monitors;
}
fn save_audio_selection(ui: &AppWindow, state: &mut AppState) {
    state.settings.monitor_enabled = ui.get_monitor_enabled();
    state.settings.monitor_device = if ui.get_monitor_index() == 0 {
        String::new()
    } else {
        state
            .monitor_outputs
            .get((ui.get_monitor_index() - 1) as usize)
            .map(|d| d.id.clone())
            .unwrap_or_else(|| state.settings.monitor_device.clone())
    };
    state.last_edit = Some(Instant::now());
    state.settings.mic_device = match ui.get_mic_index() {
        0 => String::new(),
        1 => "asio".into(),
        i => state
            .inputs
            .get((i - 2) as usize)
            .map(|d| d.id.clone())
            .unwrap_or_else(|| state.settings.mic_device.clone()),
    };
    state.settings.mic_channel = ui.get_mic_channel().max(1.) as usize - 1;
    state.settings.output_device = if ui.get_output_index() == 0 {
        String::new()
    } else {
        state
            .outputs
            .get((ui.get_output_index() - 1) as usize)
            .map(|d| d.id.clone())
            .unwrap_or_else(|| state.settings.output_device.clone())
    };
    ui.set_virtual_output(state.settings.output_device == audio::VIRTUAL_OUTPUT);
    if let Some(device) = state.devices.get(ui.get_device_index() as usize) {
        state.settings.device = device.name.clone();
        if let Some(rate) = device.rates.get(ui.get_rate_index() as usize) {
            state.settings.sample_rate = *rate;
        }
        if let Some(buffer) = device.buffers.get(ui.get_buffer_index() as usize) {
            state.settings.buffer = *buffer;
        }
        state.settings.input_channel = ui.get_channel_index().max(0) as usize;
        state.last_edit = Some(Instant::now());
    }
}
fn append_wave(history: &mut VecDeque<WavePoint>, point: WavePoint) {
    if history
        .back()
        .is_some_and(|last| point.sequence != last.sequence + u64::from(point.frames))
    {
        // A dropped visual update is a gap, not adjacent audio. Start a fresh
        // window rather than compressing the missing time into the waveform.
        history.clear();
    }
    if history.len() >= 400 {
        history.pop_front();
    }
    history.push_back(point);
}
fn waveform(history: &VecDeque<WavePoint>, input: bool, dark: bool) -> slint::Image {
    let width = 1000usize;
    let height = 100usize;
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(width as u32, height as u32);
    let bg = if dark { [20, 25, 28] } else { [238, 241, 242] };
    let line = if dark { [139, 224, 189] } else { [38, 118, 88] };
    let center = if dark { [53, 65, 72] } else { [203, 213, 217] };
    let bytes = buffer.make_mut_bytes();
    for px in bytes.as_chunks_mut::<3>().0 {
        px.copy_from_slice(&bg);
    }
    for x in 0..width {
        let base = (height / 2 * width + x) * 3;
        bytes[base..base + 3].copy_from_slice(&center);
    }
    let offset = 400 - history.len();
    for (i, point) in history.iter().enumerate() {
        let (lo, hi) = if input {
            (point.low_in, point.high_in)
        } else {
            (point.low_out, point.high_out)
        };
        let x = (offset + i) * width / 400;
        let next = ((offset + i + 1) * width / 400).min(width);
        let top = ((0.5 - hi.clamp(-1., 1.) * 0.46) * height as f32) as usize;
        let bottom = ((0.5 - lo.clamp(-1., 1.) * 0.46) * height as f32) as usize;
        for x in x..next.saturating_sub(1).max(x + 1) {
            for y in top.min(height - 1)..=bottom.min(height - 1) {
                let p = (y * width + x) * 3;
                bytes[p..p + 3].copy_from_slice(&line);
            }
        }
    }
    slint::Image::from_rgb8(buffer)
}

pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let demo = args.iter().any(|a| a == "--demo");
    let ui_smoke = args.iter().any(|a| a == "--ui-smoke");
    if ui_smoke && std::env::var_os("GUITAR_EFFECTER_DATA_DIR").is_none() {
        return Err("--ui-smoke requires an isolated GUITAR_EFFECTER_DATA_DIR".into());
    }
    let live_seconds = crate::argument(args, "--live-test")
        .map(str::parse::<u64>)
        .transpose()?;
    let live_buffer = crate::argument(args, "--buffer")
        .unwrap_or("32")
        .parse::<u32>()?;
    let live_report = crate::argument(args, "--report")
        .unwrap_or("artifacts/live-ui-test.json")
        .to_owned();
    let screenshot = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let initial = Settings::load();
    let load_error = match initial.as_ref().err() {
        Some(error) => {
            let backup = Settings::backup_unreadable().map_err(std::io::Error::other)?;
            Some(format!("{error}（元の設定は {} に退避）", backup.display()))
        }
        None => None,
    };
    let mut settings = initial.unwrap_or_default();
    if let Some(output) = crate::argument(args, "--output") {
        settings.output_device = output.to_owned();
    }
    if live_seconds.is_some() {
        for effect in &mut settings.tone.effects {
            effect.enabled = true;
        }
        settings.buffer = live_buffer;
    }
    let controls = Arc::new(SharedControls::new(&settings.tone));
    let (commands, events, worker) = audio::spawn_worker(controls.clone());
    let ui = AppWindow::new()?;
    ui.set_dark(settings.dark);
    ui.set_waveform_enabled(settings.waveform);
    ui.set_demo(demo);
    ui.set_output_names(strings(["同じ機器 · ASIO（低遅延）".into()]));
    ui.set_monitor_names(strings(["入力機器 · ASIO（低遅延）".into()]));
    if args.iter().any(|a| a == "--compact") {
        ui.window().set_size(slint::LogicalSize::new(900., 740.));
    }
    if live_seconds.is_some() {
        ui.set_muted(true);
        controls.mute.store(1, Ordering::Relaxed);
    }
    if args.iter().any(|a| a == "--light") {
        ui.set_dark(false);
    }
    if args.iter().any(|a| a == "--settings") {
        ui.set_settings_open(true);
    }
    if args.iter().any(|a| a == "--eq") {
        ui.set_eq_open(true);
    }
    let state = Rc::new(RefCell::new(AppState {
        settings: settings.clone(),
        controls: controls.clone(),
        devices: vec![],
        outputs: vec![],
        monitor_outputs: vec![],
        inputs: vec![],
        feed: None,
        wave: VecDeque::with_capacity(400),
        tuner: VecDeque::with_capacity(2048),
        parameters: Rc::new(VecModel::default()),
        last_edit: None,
        last_pitch: Instant::now(),
        last_clip: Instant::now() - Duration::from_secs(5),
        last_blocks: 0,
        last_audio: Instant::now(),
        demo_processor: if demo {
            Some(dsp::Processor::new(48000., settings.tone))
        } else {
            None
        },
        demo_sample: 0,
    }));
    refresh_effect(&ui, &mut state.borrow_mut());
    ui.set_mic_names(strings([
        "マイクなし".into(),
        "同じASIO機器の別入力".into(),
    ]));
    refresh_presets(&ui, &state.borrow());
    ui.set_waveform(waveform(&state.borrow().wave, false, ui.get_dark()));
    if let Some(error) = load_error {
        ui.set_message(format!("設定を読み込めませんでした: {error}").into());
    }
    if demo {
        ui.set_status("デザイン確認".into());
    } else {
        ui.set_busy(true);
        commands.send(Command::Scan)?;
    }

    macro_rules! bind {
        ($method:ident,$body:expr) => {{
            let weak = ui.as_weak();
            let state = state.clone();
            ui.$method(move |a| {
                if let Some(ui) = weak.upgrade() {
                    ($body)(&ui, &mut state.borrow_mut(), a);
                }
            });
        }};
    }
    bind!(on_select_effect, |ui: &AppWindow,
                             s: &mut AppState,
                             i: i32| {
        ui.set_selected(i.clamp(0, 6));
        refresh_effect(ui, s);
    });
    {
        let weak = ui.as_weak();
        let state = state.clone();
        ui.on_eq_changed(move || {
            if let Some(ui) = weak.upgrade() {
                refresh_effect(&ui, &mut state.borrow_mut());
            }
        });
    }
    bind!(on_toggle_effect, |ui: &AppWindow,
                             s: &mut AppState,
                             i: i32| {
        if let Some(effect) = s.settings.tone.effects.get_mut(i as usize) {
            effect.enabled = !effect.enabled;
            changed(ui, s);
            refresh_effect(ui, s);
        }
    });
    {
        let weak = ui.as_weak();
        let state = state.clone();
        ui.on_param_edited(move |index, value| {
            if let Some(ui) = weak.upgrade() {
                edit_parameter(&ui, &mut state.borrow_mut(), index, value);
            }
        });
    }
    {
        let weak = ui.as_weak();
        let state = state.clone();
        ui.on_param_numeric(move |index, text| {
            if let Some(ui) = weak.upgrade() {
                let value = text.split_whitespace().next().unwrap_or("").parse::<f32>();
                match value {
                    Ok(value) => edit_parameter(&ui, &mut state.borrow_mut(), index, value),
                    Err(_) => ui.set_message("数値を入力してください".into()),
                }
            }
        });
    }
    {
        let weak = ui.as_weak();
        let state = state.clone();
        let previous = RefCell::new(None::<Instant>);
        ui.on_tap_delay(move || {
            let now = Instant::now();
            if let Some(last) = previous.borrow_mut().replace(now) {
                let ms = now.duration_since(last).as_secs_f32() * 1000.;
                if (20.0..=1500.).contains(&ms)
                    && let Some(ui) = weak.upgrade()
                {
                    edit_parameter(&ui, &mut state.borrow_mut(), 0, ms);
                }
            }
        });
    }
    bind!(on_mode_changed, |ui: &AppWindow,
                            s: &mut AppState,
                            i: i32| {
        s.settings.tone.effects[ui.get_selected() as usize].mode = i.clamp(0, 1) as u32;
        changed(ui, s);
    });
    bind!(
        on_output_changed,
        |ui: &AppWindow, s: &mut AppState, value: f32| {
            s.settings.tone.output_db = value.clamp(-60., 0.);
            ui.set_output_db(value);
            changed(ui, s);
        }
    );
    bind!(on_input_changed, |ui: &AppWindow,
                             s: &mut AppState,
                             value: f32| {
        s.settings.tone.input_db = value.clamp(-24., 24.);
        ui.set_input_db(value);
        changed(ui, s);
    });
    bind!(
        on_monitor_changed,
        |ui: &AppWindow, s: &mut AppState, value: f32| {
            if !value.is_finite() {
                return;
            }
            s.settings.monitor_db = value.clamp(-60., 6.);
            ui.set_monitor_db(s.settings.monitor_db);
            changed(ui, s);
        }
    );
    bind!(on_bass_changed, |ui: &AppWindow,
                            s: &mut AppState,
                            bass: bool| {
        s.settings.tone.bass = bass;
        changed(ui, s);
    });
    bind!(on_bass_preset, |ui: &AppWindow,
                           s: &mut AppState,
                           drive: bool| {
        s.settings.tone = Tone::bass_factory(drive);
        ui.set_preset_index(0);
        refresh_effect(ui, s);
        changed(ui, s);
    });
    {
        let weak = ui.as_weak();
        let state = state.clone();
        ui.on_mixer_changed(move |mic, value| {
            if !value.is_finite() {
                return;
            }
            if let Some(ui) = weak.upgrade() {
                let mut s = state.borrow_mut();
                let value = value.clamp(-60., 6.);
                if mic {
                    s.settings.mic_db = value;
                    ui.set_mic_db(value);
                } else {
                    s.settings.instrument_db = value;
                    ui.set_instrument_db(value);
                }
                changed(&ui, &mut s);
            }
        });
    }
    bind!(on_device_changed, |ui: &AppWindow,
                              s: &mut AppState,
                              i: i32| {
        device_options(ui, s, i as usize);
        save_audio_selection(ui, s);
    });
    {
        let weak = ui.as_weak();
        let state = state.clone();
        ui.on_audio_config_changed(move || {
            if let Some(ui) = weak.upgrade() {
                save_audio_selection(&ui, &mut state.borrow_mut());
            }
        });
    }
    bind!(on_theme_changed, |_: &AppWindow,
                             s: &mut AppState,
                             dark: bool| {
        s.settings.dark = dark;
        s.last_edit = Some(Instant::now());
    });
    bind!(on_wave_changed, |_: &AppWindow,
                            s: &mut AppState,
                            enabled: bool| {
        s.settings.waveform = enabled;
        s.last_edit = Some(Instant::now());
    });
    bind!(on_preset_selected, |ui: &AppWindow,
                               s: &mut AppState,
                               i: i32| {
        if (1..=3).contains(&i) {
            s.settings.tone = Tone::factory((i - 1) as usize);
        } else if i >= 4
            && let Some(preset) = s.settings.presets.get((i - 4) as usize)
        {
            s.settings.tone = preset.tone;
            ui.set_preset_name(preset.name.clone().into());
        }
        changed(ui, s);
        refresh_effect(ui, s);
        ui.set_dirty(false);
        ui.set_user_preset(i >= 4);
    });
    {
        let weak = ui.as_weak();
        let state = state.clone();
        ui.on_preset_action(move |action, name| {
            if let Some(ui) = weak.upgrade() {
                let mut s = state.borrow_mut();
                let name: String = name.trim().chars().take(40).collect();
                let index = ui
                    .get_preset_index()
                    .checked_sub(4)
                    .filter(|v| *v >= 0)
                    .map(|v| v as usize);
                if action != 3 && name.is_empty() {
                    ui.set_message("名前を入力してください".into());
                    return;
                }
                if matches!(action, 0 | 2)
                    && s.settings
                        .presets
                        .iter()
                        .enumerate()
                        .any(|(i, p)| p.name == name && (action == 0 || Some(i) != index))
                {
                    ui.set_message("同じ名前があるため、別の名前を入力してください".into());
                    return;
                }
                let tone = s.settings.tone;
                match action {
                    0 => {
                        if s.settings.presets.len() >= 128 {
                            ui.set_message("プリセットは128件までです".into());
                            return;
                        }
                        s.settings.presets.push(Preset { name, tone });
                        let index = s.settings.presets.len() + 3;
                        refresh_presets(&ui, &s);
                        ui.set_preset_index(index as i32);
                    }
                    1 => {
                        if let Some(p) = index.and_then(|i| s.settings.presets.get_mut(i)) {
                            p.tone = tone;
                        }
                    }
                    2 => {
                        if let Some(p) = index.and_then(|i| s.settings.presets.get_mut(i)) {
                            p.name = name;
                        }
                    }
                    3 => {
                        if let Some(i) = index.filter(|i| *i < s.settings.presets.len()) {
                            s.settings.presets.remove(i);
                            ui.set_preset_index(0);
                        }
                    }
                    _ => return,
                }
                refresh_presets(&ui, &s);
                match s.settings.save() {
                    Ok(_) => {
                        ui.set_message("保存しました".into());
                        ui.set_dirty(false);
                        ui.set_presets_open(false);
                        s.last_edit = None;
                    }
                    Err(e) => ui.set_message(format!("保存できません: {e}").into()),
                }
            }
        });
    }
    {
        let weak = ui.as_weak();
        let state = state.clone();
        let commands = commands.clone();
        ui.on_toggle_engine(move || {
            if let Some(ui) = weak.upgrade() {
                if ui.get_busy() {
                    return;
                }
                if ui.get_running() {
                    ui.set_busy(true);
                    let _ = commands.send(Command::Stop);
                    return;
                }
                let mut s = state.borrow_mut();
                let Some(device) = s.devices.get(ui.get_device_index() as usize).cloned() else {
                    ui.set_settings_open(true);
                    ui.set_message("ASIO機器を接続して「再検出」を押してください".into());
                    return;
                };
                let Some(rate) = device.rates.get(ui.get_rate_index() as usize) else {
                    return;
                };
                let Some(buffer) = device.buffers.get(ui.get_buffer_index() as usize) else {
                    return;
                };
                s.settings.device = device.name.clone();
                s.settings.sample_rate = *rate;
                s.settings.buffer = *buffer;
                s.settings.input_channel = ui.get_channel_index().max(0) as usize;
                s.controls
                    .mute
                    .store(u32::from(ui.get_muted()), Ordering::Relaxed);
                s.controls
                    .bypass
                    .store(u32::from(ui.get_bypassed()), Ordering::Relaxed);
                s.controls.publish(&s.settings.tone);
                ui.set_message("".into());
                ui.set_busy(true);
                ui.set_status("ASIOを開始中…".into());
                let _ = commands.send(Command::Start(Box::new(s.settings.clone())));
                s.last_edit = Some(Instant::now());
            }
        });
    }
    {
        let weak = ui.as_weak();
        let state = state.clone();
        ui.on_select_virtual_output(move || {
            if let Some(ui) = weak.upgrade() {
                if ui.get_running() || ui.get_busy() {
                    return;
                }
                let mut s = state.borrow_mut();
                if let Some(index) = s.outputs.iter().position(|d| d.id == audio::VIRTUAL_OUTPUT) {
                    ui.set_output_index((index + 1) as i32);
                    save_audio_selection(&ui, &mut s);
                }
            }
        });
    }
    ui.on_open_virtual_help(|| {
        let _ = std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", audio::VIRTUAL_OUTPUT_HELP])
            .spawn();
    });
    {
        let weak = ui.as_weak();
        let commands = commands.clone();
        ui.on_rescan(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_busy(true);
                let _ = commands.send(Command::Scan);
            }
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_open_driver_help(move || {
            let yamaha = weak.upgrade().is_some_and(|ui| {
                let name = ui.get_device_label().to_lowercase();
                name.contains("yamaha") || name.contains("steinberg") || name.contains("ag03")
            });
            let _ = std::process::Command::new("rundll32.exe")
                .args(["url.dll,FileProtocolHandler", if yamaha {
                    "https://jp.yamaha.com/products/proaudio/live_streaming_gaming/ag/ag03mk2/downloads.html"
                } else { "https://www.m-audio.com/support/downloads" }]).spawn();
        });
    }

    let timer = slint::Timer::default();
    let weak = ui.as_weak();
    let timer_state = state.clone();
    let timer_controls = controls.clone();
    let timer_commands = commands.clone();
    let mut live_start = None::<Instant>;
    let mut live_last_progress = Instant::now();
    let mut live_step = 0u64;
    timer.start(slint::TimerMode::Repeated,Duration::from_millis(33),move||{let Some(ui)=weak.upgrade()else{return;};let mut s=timer_state.borrow_mut();
        while let Ok(event)=events.try_recv(){match event{
            Event::Devices(result)=>{ui.set_busy(false);match result{Ok(devices)=>{s.devices=devices;ui.set_device_names(strings(s.devices.iter().map(|d|d.name.clone())));let index=s.devices.iter().position(|d|d.name==s.settings.device).or_else(||s.devices.iter().position(|d|d.name.contains("M-Track"))).unwrap_or(0);ui.set_device_index(index as i32);device_options(&ui,&s,index);if s.devices.is_empty(){ui.set_status("ASIO機器なし".into());ui.set_message("AG03MK2 / M-TrackのUSB接続と公式ASIOドライバーを確認してください".into());}else{ui.set_status("接続待ち".into());}},Err(e)=>{ui.set_status("ASIO確認エラー".into());ui.set_message(e.into());}}}
            Event::Inputs(result)=>{match result {
                Ok(mut inputs)=>{
                    if !s.settings.mic_device.is_empty() && s.settings.mic_device != "asio" && !inputs.iter().any(|d| d.id == s.settings.mic_device) {
                        inputs.push(audio::OutputDeviceInfo { id: s.settings.mic_device.clone(), name: "未接続のマイク".into() });
                    }
                    ui.set_mic_names(strings(["マイクなし".into(), "同じASIO機器の別入力".into()].into_iter().chain(inputs.iter().map(|d| d.name.clone()))));
                    ui.set_mic_index(if s.settings.mic_device == "asio" {1} else { inputs.iter().position(|d| d.id == s.settings.mic_device).map(|i|i+2).unwrap_or(0) as i32 });
                    ui.set_mic_channel((s.settings.mic_channel+1) as f32);
                    s.inputs=inputs;
                }, Err(e)=>ui.set_message(e.into())
            }}
            Event::Outputs(result)=>{match result {
                Ok(mut outputs)=>{
                    if !s.settings.output_device.is_empty() && !outputs.iter().any(|d|d.id==s.settings.output_device) {
                        outputs.push(audio::OutputDeviceInfo{id:s.settings.output_device.clone(),name:"未接続の出力（選び直してください）".into()});
                    }
                    ui.set_output_names(strings(std::iter::once("同じ機器 · ASIO（低遅延）".to_owned()).chain(outputs.iter().map(|d|d.name.clone()))));
                    ui.set_output_index(outputs.iter().position(|d|d.id==s.settings.output_device).map(|i|i+1).unwrap_or(0) as i32);
                    ui.set_virtual_output(s.settings.output_device == audio::VIRTUAL_OUTPUT);
                    refresh_monitor_outputs(&ui, &mut s, &outputs);
                    s.outputs=outputs;
                },
                Err(error)=>ui.set_message(format!("Windows出力を列挙できません: {error}").into())
            }}
            Event::Started(result)=>{ui.set_busy(false);match result{Ok((feed,info))=>{s.feed=Some(feed);s.wave.clear();s.tuner.clear();s.last_blocks=0;s.last_audio=Instant::now();ui.set_running(true);ui.set_status(if s.settings.output_device.is_empty(){"ASIO 接続中"}else if s.settings.output_device == audio::VIRTUAL_OUTPUT {"仮想出力 接続中"}else{"Windows出力 接続中"}.into());ui.set_device_label(info.into());ui.set_settings_open(false);if live_seconds.is_some(){live_start=Some(Instant::now());}},Err(e)=>{ui.set_running(false);ui.set_status("開始できません".into());ui.set_message(e.into());ui.set_settings_open(true);}}}
            Event::Stopped=>{s.feed=None;s.wave.clear();s.tuner.clear();ui.set_running(false);ui.set_busy(false);ui.set_status("停止中".into());ui.set_latency("—".into());ui.set_input_level(0.);ui.set_output_level(0.);}
        }}
        if live_seconds.is_some()&&live_start.is_none()&&!ui.get_busy()&&!s.devices.is_empty(){let device=s.devices[ui.get_device_index() as usize].clone();s.settings.device=device.name;s.settings.buffer=live_buffer;ui.set_busy(true);let _=timer_commands.send(Command::Start(Box::new(s.settings.clone())));}
        timer_controls.mute.store(u32::from(ui.get_busy()||ui.get_muted()||(ui.get_tuner_open()&&ui.get_tuner_muted())),Ordering::Relaxed);timer_controls.bypass.store(u32::from(ui.get_bypassed()),Ordering::Relaxed);
        if let Some(mut feed)=s.feed.take(){
            while let Ok(point)=feed.wave.pop(){append_wave(&mut s.wave,point);}
            while let Ok(sample)=feed.tuner.pop(){if s.tuner.len()>=2048{s.tuner.pop_front();}s.tuner.push_back(sample);}
            let m=&feed.metrics;let input=f32::from_bits(m.input_peak.swap(0,Ordering::Relaxed));let output=f32::from_bits(m.output_peak.swap(0,Ordering::Relaxed));
            ui.set_input_level(input.max(ui.get_input_level()*0.85));ui.set_output_level(output.max(ui.get_output_level()*0.85));
            if input>=0.999||output>=1.{s.last_clip=Instant::now();ui.set_input_clip(input>=0.999);ui.set_output_clip(output>=1.);}else if s.last_clip.elapsed()>Duration::from_secs(1){ui.set_input_clip(false);ui.set_output_clip(false);}
            let report=m.report();ui.set_latency(format!("{:.2} ms",report.estimated_round_trip_ms).into());ui.set_diagnostics(format!("実バッファ {} samples · 処理最大 {} µs\n入出力欠落 {} / {} frames · 期限超過 {} · エラー {}\nマイク: 待ち {:.1} ms · 欠落 {} / {} frames · 一時欠落 {} · エラー {}",report.callback_frames,report.max_process_us,report.missing_frames,report.overflow_frames,report.deadline_misses,report.errors,report.mic_queue_ms,report.mic_missing_frames,report.mic_overflow_frames,report.mic_discontinuities,report.mic_errors).into());
            if let Some(monitor) = &feed.monitor_metrics {
                let monitor = monitor.report();
                ui.set_diagnostics(format!("{}\nベースモニター: 推定 {:.2} ms · 欠落 {} / {} frames · エラー {}",ui.get_diagnostics(),report.driver_input_ms + monitor.driver_output_ms + monitor.callback_queue_ms,monitor.missing_frames,monitor.overflow_frames,monitor.errors).into());
                if monitor.errors > 0 && !ui.get_busy() {
                    ui.set_message("ベースモニター出力でエラーが発生しました。接続を確認して再接続してください。".into());
                }
            }
            if report.mic_errors > 0 && !ui.get_busy() {
                ui.set_message(format!("{}。楽器出力は継続中です。停止して再接続してください。",report.mic_error.unwrap_or("マイク入力エラー")).into());
            }
            if report.input_blocks!=s.last_blocks{s.last_blocks=report.input_blocks;s.last_audio=Instant::now();}
            if (report.errors>0||s.last_audio.elapsed()>Duration::from_secs(2))&&!ui.get_busy(){ui.set_message("音声接続が停止しました。接続を確認して、再接続してください。".into());ui.set_busy(true);let _=timer_commands.send(Command::Stop);}
            if ui.get_tuner_open()&&s.last_pitch.elapsed()>Duration::from_millis(150){let samples:Vec<_>=s.tuner.iter().copied().collect();update_pitch(&ui,&samples,feed.rate as f32/4.);s.last_pitch=Instant::now();}
            if let (Some(start),Some(seconds))=(live_start,live_seconds){
                let elapsed=start.elapsed();let step=elapsed.as_secs();
                if step!=live_step{live_step=step;ui.set_selected((step%7) as i32);refresh_effect(&ui,&mut s);s.settings.tone.effects[2].params[0]=20.+(step%60)as f32;timer_controls.publish(&s.settings.tone);ui.set_wave_input(step%8<4);ui.set_settings_open(step%20>=17);}
                if live_last_progress.elapsed()>Duration::from_secs(30)||elapsed.as_secs()>=seconds{
                    let value=serde_json::json!({"duration_seconds":elapsed.as_secs_f64(),"sample_rate":feed.rate,"requested_buffer":live_buffer,"all_effects":true,"ui_running":true,"waveform_enabled":ui.get_waveform_enabled(),"muted_output":true,"metrics":report});
                    let path=if elapsed.as_secs()>=seconds{live_report.clone()}else{format!("{live_report}.progress.json")};let _=std::fs::write(path,serde_json::to_vec_pretty(&value).unwrap());live_last_progress=Instant::now();
                }
                if elapsed.as_secs()>=seconds{let _=slint::quit_event_loop();}
            }
            s.feed=Some(feed);
        }
        if demo {
            let tone=s.settings.tone;let mut processor=s.demo_processor.take().unwrap();processor.set_tone(tone);
            for _ in 0..7{let mut point=WavePoint::default();for _ in 0..240{let n=s.demo_sample;let t=n as f32/48000.;let envelope=(-(t%0.62)*6.).exp();let input=((std::f32::consts::TAU*110.*t).sin()+0.3*(std::f32::consts::TAU*220.*t).sin())*0.22*envelope;let(input,out)=processor.process(input,ui.get_bypassed(),ui.get_muted());point.low_in=point.low_in.min(input);point.high_in=point.high_in.max(input);point.low_out=point.low_out.min(out[0]).min(out[1]);point.high_out=point.high_out.max(out[0]).max(out[1]);s.demo_sample+=1;}point.sequence=s.demo_sample;point.frames=240;append_wave(&mut s.wave,point);}s.demo_processor=Some(processor);
        }
        if ui.get_waveform_enabled()&&!ui.window().is_minimized(){ui.set_waveform(waveform(&s.wave,ui.get_wave_input(),ui.get_dark()));}
        if !demo&&live_seconds.is_none()&&s.last_edit.is_some_and(|time|time.elapsed()>Duration::from_millis(800)){if let Err(e)=s.settings.save(){ui.set_message(format!("設定の保存に失敗: {e}").into());}s.last_edit=None;}
    });
    let smoke_timer = slint::Timer::default();
    if ui_smoke {
        let outputs = audio::windows_outputs().map_err(std::io::Error::other)?;
        ui.set_output_names(strings(
            std::iter::once("同じ機器 · ASIO（低遅延）".to_owned())
                .chain(outputs.iter().map(|d| d.name.clone())),
        ));
        refresh_monitor_outputs(&ui, &mut state.borrow_mut(), &outputs);
        state.borrow_mut().outputs = outputs;
        let weak = ui.as_weak();
        smoke_timer.start(slint::TimerMode::SingleShot,Duration::from_millis(1500),move||{
        let ui=weak.upgrade().unwrap();
        let click=|x,y|{let position=slint::LogicalPosition::new(x,y);ui.window().dispatch_event(slint::platform::WindowEvent::PointerPressed{position,button:slint::platform::PointerEventButton::Left});ui.window().dispatch_event(slint::platform::WindowEvent::PointerReleased{position,button:slint::platform::PointerEventButton::Left});};
        click(406.,302.);assert_eq!(ui.get_selected(),2,"effect selection must work by pointer");
        let before=ui.get_effects().row_data(2).unwrap().enabled;click(406.,334.);assert_ne!(ui.get_effects().row_data(2).unwrap().enabled,before,"on/off must be separate from selection");
        // Exercise the native pointer/key path, not only callback invocation.
        ui.invoke_param_edited(0,40.);
        let start=slint::LogicalPosition::new(416.,550.);
        let end=slint::LogicalPosition::new(416.,514.);
        ui.window().dispatch_event(slint::platform::WindowEvent::PointerPressed{position:start,button:slint::platform::PointerEventButton::Left});
        ui.window().dispatch_event(slint::platform::WindowEvent::PointerMoved{position:end});
        ui.window().dispatch_event(slint::platform::WindowEvent::PointerReleased{position:end,button:slint::platform::PointerEventButton::Left});
        let dragged=ui.get_parameters().row_data(0).unwrap().value;
        assert!(dragged>40.,"native knob drag must change gain");
        ui.window().dispatch_event(slint::platform::WindowEvent::KeyPressed{text:slint::platform::Key::UpArrow.into()});
        ui.window().dispatch_event(slint::platform::WindowEvent::KeyReleased{text:slint::platform::Key::UpArrow.into()});
        assert_eq!(ui.get_parameters().row_data(0).unwrap().value,dragged+1.,"focused knob keyboard adjustment");
        ui.invoke_param_edited(0,73.);assert_eq!(ui.get_parameters().row_data(0).unwrap().value,73.);
        ui.invoke_param_numeric(1,"42 %".into());assert_eq!(ui.get_parameters().row_data(1).unwrap().value,42.);
        click(984.,52.);assert!(ui.get_muted(),"mute button must respond");click(984.,52.);assert!(!ui.get_muted());
        ui.invoke_select_effect(3);
        assert_eq!(ui.get_parameters().row_count(),2);
        ui.invoke_param_numeric(4,"-9 dB".into());
        assert_eq!(ui.get_parameters().row_data(1).unwrap().value,-9.);
        ui.set_eq_open(true);ui.invoke_eq_changed();
        assert_eq!(ui.get_parameters().row_count(),5);
        assert_eq!(ui.get_parameters().row_data(4).unwrap().value,-9.);
        ui.set_eq_open(false);ui.invoke_eq_changed();
        ui.invoke_bass_preset(true);assert!(ui.get_bass_mode());
        ui.invoke_mixer_changed(true,-18.);assert_eq!(ui.get_mic_db(),-18.);
        ui.invoke_mixer_changed(false,-6.);assert_eq!(ui.get_instrument_db(),-6.);
        ui.set_mic_index(1);ui.set_mic_channel(1.);ui.invoke_audio_config_changed();
        ui.invoke_preset_selected(3);assert!(ui.get_effects().row_data(5).unwrap().enabled);
        ui.set_output_index(1);ui.invoke_audio_config_changed();
        ui.invoke_preset_action(0,"UI smoke tone".into());assert_eq!(Settings::load().unwrap().output_device,"windows:default");assert_eq!(Settings::load().unwrap().mic_device,"asio");assert_eq!(Settings::load().unwrap().mic_db,-18.);assert!(Settings::load().unwrap().presets.iter().any(|p|p.name=="UI smoke tone"));
        ui.invoke_preset_action(2,"Renamed smoke tone".into());assert!(Settings::load().unwrap().presets.iter().any(|p|p.name=="Renamed smoke tone"));
        ui.invoke_preset_action(3,"".into());assert!(!Settings::load().unwrap().presets.iter().any(|p|p.name=="Renamed smoke tone"));
        ui.invoke_select_virtual_output();assert!(ui.get_virtual_output());
        ui.set_monitor_enabled(true);ui.set_monitor_index(1);ui.invoke_audio_config_changed();ui.invoke_monitor_changed(-9.);

        ui.invoke_preset_action(0,"Virtual route smoke".into());
        assert_eq!(Settings::load().unwrap().output_device,audio::VIRTUAL_OUTPUT);
        assert_eq!(Settings::load().unwrap().monitor_device,"windows:default");assert!(Settings::load().unwrap().monitor_enabled);assert_eq!(Settings::load().unwrap().monitor_db,-9.);
        ui.invoke_preset_action(3,"".into());
        ui.set_message("".into());ui.set_settings_open(false);ui.set_dark(true);save_png("artifacts/ui-smoke-dark.png",&ui.window().take_snapshot().unwrap()).unwrap();
        ui.set_dark(false);ui.set_settings_open(true);save_png("artifacts/ui-smoke-settings-light.png",&ui.window().take_snapshot().unwrap()).unwrap();
        ui.set_settings_open(false);ui.set_tuner_open(true);save_png("artifacts/ui-smoke-tuner.png",&ui.window().take_snapshot().unwrap()).unwrap();
        std::fs::write("artifacts/ui-smoke.json",r#"{"passed":true,"checks":["pointer effect selection","pointer independent bypass","parameter edit","numeric edit","pointer mute","knob drag","knob keyboard","collapsed amp level mapping","EQ expansion","Windows output selection persists","virtual output selection persists","bass monitor settings persist","factory preset","bass preset","mixer gain and microphone selection persistence","save and reload","rename and reload","delete and reload","dark light and tuner snapshots"]}"#).unwrap();
        let _=slint::quit_event_loop();
    });
    }
    let screenshot_timer = slint::Timer::default();
    if let Some(path) = screenshot {
        let weak = ui.as_weak();
        screenshot_timer.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(if demo { 2500 } else { 1500 }),
            move || {
                if let Some(ui) = weak.upgrade() {
                    match ui.window().take_snapshot() {
                        Ok(image) => {
                            let result = save_png(&path, &image);
                            if let Err(e) = result {
                                eprintln!("Screenshot failed: {e}");
                            }
                        }
                        Err(e) => eprintln!("Screenshot failed: {e}"),
                    };
                    let _ = slint::quit_event_loop();
                }
            },
        );
    }
    ui.run()?;
    timer.stop();
    controls.mute.store(1, Ordering::Relaxed);
    let _ = commands.send(Command::Shutdown);
    let _ = worker.join();
    if !demo && live_seconds.is_none() {
        state
            .borrow()
            .settings
            .save()
            .map_err(std::io::Error::other)?;
    }
    Ok(())
}
fn update_pitch(ui: &AppWindow, samples: &[f32], rate: f32) {
    if let Some((name, cents, hz)) = dsp::pitch(samples, rate) {
        ui.set_tuner_note(name.into());
        ui.set_tuner_cents(cents);
        ui.set_tuner_detail(format!("{hz:.1} Hz · {cents:+.0} cent").into());
    } else {
        ui.set_tuner_note("—".into());
        ui.set_tuner_cents(0.);
        ui.set_tuner_detail("弦を1本鳴らしてください".into());
    }
}
fn save_png(
    path: &str,
    image: &slint::SharedPixelBuffer<slint::Rgba8Pixel>,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(file, image.width(), image.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(image.as_bytes())?;
    Ok(())
}
