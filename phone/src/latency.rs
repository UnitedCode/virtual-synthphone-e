//! Local acoustic loopback analysis. Neither browser latency hints nor engine
//! throughput count as a measurement. All three streams share the worklet clock.
const START: f64 = 0.3;
const SPACING: f64 = 0.5;
const PULSES: usize = 5;
const WIDTH: f64 = 0.064;
pub const DURATION: f64 = 3.2;

pub fn probe(rate: f64) -> Vec<f32> {
    let mut result = vec![0.; (rate * DURATION).ceil() as usize];
    for pulse in 0..PULSES {
        let start = ((START + pulse as f64 * SPACING) * rate).round() as usize;
        let mut random = 0x7193_abedu32;
        let mut amplitude = 0.;
        for i in 0..(WIDTH * rate).round() as usize {
            if i % (rate * 0.004).round() as usize == 0 {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                amplitude = if random & 1 == 0 { 0.12 } else { 1. };
            }
            let time = i as f64 / rate;
            let edge = (time / 0.001).min((WIDTH - time) / 0.001).clamp(0., 1.);
            let carrier: f64 = [431., 733., 1091., 1553., 2111.]
                .iter()
                .enumerate()
                .map(|(j, hz)| (std::f64::consts::TAU * hz * time + j as f64).sin())
                .sum();
            result[start + i] = (carrier * 0.012 * amplitude * edge) as f32;
        }
    }
    result
}

#[derive(Debug)]
pub struct Measurement {
    /// Output device + air path + input device, including browser capture.
    pub device_ms: f64,
    /// Raw worklet mic arrival to processed firmware audio ready for rendering.
    pub processing_ms: f64,
    pub spread_ms: f64,
    pub confidence: f64,
}
impl Measurement {
    pub fn total_ms(&self) -> f64 {
        self.device_ms + self.processing_ms
    }
}

fn envelope(samples: &[f32], bin: usize) -> Vec<f64> {
    samples
        .chunks_exact(bin)
        .map(|s| (s.iter().map(|&v| f64::from(v).powi(2)).sum::<f64>() / bin as f64).sqrt())
        .collect()
}
fn correlation(template: &[f64], samples: &[f64]) -> f64 {
    let a = template.iter().sum::<f64>() / template.len() as f64;
    let b = samples.iter().sum::<f64>() / samples.len() as f64;
    let (mut xy, mut xx, mut yy) = (0., 0., 0.);
    for (x, y) in template.iter().zip(samples) {
        let (x, y) = (x - a, y - b);
        xy += x * y;
        xx += x * x;
        yy += y * y;
    }
    if xx < 1e-12 || yy < 1e-12 {
        0.
    } else {
        xy / (xx * yy).sqrt()
    }
}
fn locate(template: &[f64], samples: &[f64], start: usize, range: usize) -> Option<(usize, f64)> {
    (start..=start + range)
        .filter(|&i| i + template.len() <= samples.len())
        .map(|i| (i, correlation(template, &samples[i..i + template.len()])))
        .max_by(|a, b| a.1.total_cmp(&b.1))
}
fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

pub fn measure(
    reference: &[f32],
    mic: &[f32],
    processed: &[f32],
    rate: f64,
    underruns: u32,
) -> Result<Measurement, &'static str> {
    if !(8_000. ..=192_000.).contains(&rate)
        || !rate.is_finite()
        || reference.len() != mic.len()
        || mic.len() != processed.len()
        || mic.len() < (DURATION * rate).ceil() as usize
        || reference
            .iter()
            .chain(mic)
            .chain(processed)
            .any(|v| !v.is_finite())
    {
        return Err("Incomplete audio capture. Try again.");
    }
    if underruns != 0 {
        return Err("Audio dropped during the test. Close other audio work and try again.");
    }
    if mic.iter().any(|v| v.abs() >= 0.99) {
        return Err("Microphone clipped. Lower the speaker volume and try again.");
    }
    let bin = (rate * 0.0005).round() as usize;
    let seconds = bin as f64 / rate;
    let reference = envelope(reference, bin);
    let mic = envelope(mic, bin);
    let processed = envelope(processed, bin);
    let width = (WIDTH / seconds).round() as usize;
    let range = (0.25 / seconds).round() as usize;
    let mut device = Vec::new();
    let mut app = Vec::new();
    let mut confidence: f64 = 1.;
    for pulse in 0..PULSES {
        let start = ((START + pulse as f64 * SPACING) / seconds).round() as usize;
        let (arrival, score) = locate(&reference[start..start + width], &mic, start, range)
            .ok_or("No microphone probe detected.")?;
        if score < 0.7 {
            return Err(
                "No clear speaker-to-mic signal. Use speakers, a live mic and a quiet room.",
            );
        }
        let (render, match_score) =
            locate(&mic[arrival..arrival + width], &processed, arrival, range)
                .ok_or("No firmware output detected.")?;
        if match_score < 0.7 {
            return Err("Firmware output did not match. Try Dry mode with the mic level up and MIDI stopped.");
        }
        confidence = confidence.min(score).min(match_score);
        device.push((arrival - start) as f64 * seconds * 1000.);
        app.push((render - arrival) as f64 * seconds * 1000.);
    }
    let device_ms = median(&mut device);
    let processing_ms = median(&mut app);
    let spread_ms = (device[PULSES - 1] - device[0]).max(app[PULSES - 1] - app[0]);
    if spread_ms > 4. {
        return Err("Delay changed during the test. Let the phone settle, then retry.");
    }
    Ok(Measurement {
        device_ms,
        processing_ms,
        spread_ms,
        confidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn delayed(input: &[f32], delay: usize, gain: f32) -> Vec<f32> {
        let mut result = vec![0.; input.len()];
        for (i, &v) in input.iter().take(input.len() - delay).enumerate() {
            result[i + delay] = v * gain;
        }
        result
    }
    #[test]
    fn separates_device_and_firmware_delays_across_browser_rates() {
        for rate in [44100., 48000., 96000.] {
            let reference = probe(rate);
            let mic = delayed(&reference, (rate * 0.023) as usize, 0.3);
            let processed = delayed(&mic, (rate * 0.041) as usize, 0.6);
            let result = measure(&reference, &mic, &processed, rate, 0).unwrap();
            assert!((result.device_ms - 23.).abs() < 0.6, "{result:?}");
            assert!((result.processing_ms - 41.).abs() < 0.6, "{result:?}");
            assert!((result.total_ms() - 64.).abs() < 1.);
        }
    }
    #[test]
    fn rejects_silence_dropouts_and_unrelated_audio() {
        let reference = probe(48000.);
        let silence = vec![0.; reference.len()];
        assert!(measure(&reference, &silence, &silence, 48000., 0).is_err());
        assert!(measure(&reference, &reference, &reference, 48000., 1).is_err());
        let noise: Vec<_> = (0..reference.len())
            .map(|i| ((i as f64 * 1.312).sin() * 0.01) as f32)
            .collect();
        assert!(measure(&reference, &noise, &noise, 48000., 0).is_err());
    }
    #[test]
    fn rejects_time_varying_latency_and_missing_processed_audio() {
        let reference = probe(48000.);
        let mic = delayed(&reference, 1000, 0.4);
        let mut processed = delayed(&mic, 2000, 0.5);
        let tail = delayed(&mic, 3000, 0.5);
        processed[72000..].copy_from_slice(&tail[72000..]);
        assert!(measure(&reference, &mic, &processed, 48000., 0).is_err());
        assert!(measure(&reference, &mic, &vec![0.; mic.len()], 48000., 0).is_err());
    }
}
