//! UI-thread oscilloscope. The audio callback only sends decimated instrument samples.
use crate::audio::WavePoint;
use std::collections::VecDeque;

pub fn append(history: &mut VecDeque<WavePoint>, point: WavePoint) {
    if history
        .back()
        .is_some_and(|last| point.sequence != last.sequence + u64::from(point.frames))
    {
        history.clear();
    }
    if history.len() >= 4096 {
        history.pop_front();
    }
    history.push_back(point);
}

fn trigger(samples: &[f32], count: usize) -> Option<usize> {
    if samples.len() < count + 2 {
        return None;
    }
    let peak = samples.iter().fold(0f32, |p, v| p.max(v.abs()));
    if peak < 0.0001 {
        return None;
    }
    let threshold = peak * 0.05;
    // Latest complete rising-edge capture, with hysteresis against near-zero noise.
    let mut armed = false;
    let mut found = None;
    for (i, &sample) in samples.iter().take(samples.len() - count).enumerate() {
        if sample < -threshold {
            armed = true;
        }
        if armed && sample >= 0. {
            found = Some(i);
            armed = false;
        }
    }
    found
}

pub fn image(
    history: &VecDeque<WavePoint>,
    input: bool,
    rate: u32,
    milliseconds: u32,
    line: slint::Color,
    grid: slint::Color,
) -> slint::Image {
    let width = 1000usize;
    let height = 180usize;
    let mut buffer =
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(width as u32, height as u32);
    let bytes = buffer.make_mut_bytes();
    let mut put = |x: usize, y: usize, color: slint::Color| {
        let offset = (y.min(height - 1) * width + x.min(width - 1)) * 4;
        bytes[offset..offset + 4].copy_from_slice(&[
            color.red(),
            color.green(),
            color.blue(),
            color.alpha(),
        ]);
    };
    for y in 0..height {
        for x in 0..width {
            if x % 100 == 0 || y % 30 == 0 {
                put(x, y, grid);
            }
        }
    }
    let samples: Vec<f32> = history
        .iter()
        .map(|p| if input { p.high_in } else { p.high_out })
        .collect();
    let count = ((rate as usize * milliseconds as usize) / 4000).max(2);
    let start = trigger(&samples, count);
    let mut previous = height / 2;
    for x in 0..width {
        let sample = start
            .map(|start| {
                let position = x as f32 / (width - 1) as f32 * (count - 1) as f32;
                let i = position as usize;
                let a = samples[start + i];
                let b = samples[(start + i + 1).min(samples.len() - 1)];
                a + (b - a) * position.fract()
            })
            .unwrap_or(0.);
        let y = ((0.5 - sample.clamp(-1., 1.) * 0.46) * (height - 1) as f32) as usize;
        if x == 0 {
            previous = y;
        }
        for yy in previous.min(y)..=previous.max(y) {
            put(x, yy, line);
        }
        previous = y;
    }
    slint::Image::from_rgba8(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trigger_finds_complete_positive_edge_and_rejects_silence() {
        let samples: Vec<_> = (0..2000).map(|i| (i as f32 * 0.1).sin()).collect();
        let i = trigger(&samples, 480).unwrap();
        assert!(samples[i - 1] < 0. && samples[i] >= 0.);
        assert!(i + 480 < samples.len());
        assert_eq!(trigger(&[0.; 2000], 480), None);
    }
    #[test]
    fn dropped_samples_restart_scope_history() {
        let mut h = VecDeque::new();
        append(
            &mut h,
            WavePoint {
                sequence: 4,
                frames: 4,
                ..Default::default()
            },
        );
        append(
            &mut h,
            WavePoint {
                sequence: 12,
                frames: 4,
                ..Default::default()
            },
        );
        assert_eq!(h.len(), 1);
    }
}
