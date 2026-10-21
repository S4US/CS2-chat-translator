use std::f64::consts::PI;

/// Streaming windowed-sinc resampler for mono audio.
///
/// Downsampling needs a low-pass filter below the output Nyquist frequency, so
/// the kernel cutoff scales with the rate ratio.
pub struct Resampler {
    step: f64,
    cutoff: f64,
    half_taps: usize,
    buffer: Vec<f32>,
    pos: f64,
}

impl Resampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        let step = f64::from(input_rate) / f64::from(output_rate);
        let scale = step.max(1.0);
        let half_taps = (16.0 * scale).ceil() as usize;
        Self {
            step,
            cutoff: 0.45 / scale,
            half_taps,
            buffer: vec![0.0; half_taps],
            pos: half_taps as f64,
        }
    }

    pub fn process(&mut self, input: &[f32], output: &mut Vec<f32>) {
        self.buffer.extend_from_slice(input);
        let half_taps = self.half_taps as isize;

        loop {
            let center = self.pos.floor() as isize;
            if center + half_taps >= self.buffer.len() as isize {
                break;
            }

            let frac = self.pos - center as f64;
            let mut acc = 0.0;
            for k in (1 - half_taps)..=half_taps {
                let sample = self.buffer[(center + k) as usize];
                acc += f64::from(sample) * self.kernel(k as f64 - frac);
            }
            output.push(acc as f32);
            self.pos += self.step;
        }

        let consumed = (self.pos.floor() as usize).saturating_sub(self.half_taps);
        self.buffer.drain(..consumed);
        self.pos -= consumed as f64;
    }

    fn kernel(&self, t: f64) -> f64 {
        let x = 2.0 * self.cutoff * t;
        let sinc = if x.abs() < 1e-9 {
            1.0
        } else {
            (PI * x).sin() / (PI * x)
        };
        let window = 0.5 * (1.0 + (PI * t / self.half_taps as f64).cos());
        2.0 * self.cutoff * sinc * window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, freq: f64, seconds: f64) -> Vec<f32> {
        let count = (f64::from(rate) * seconds) as usize;
        (0..count)
            .map(|i| (2.0 * PI * freq * i as f64 / f64::from(rate)).sin() as f32)
            .collect()
    }

    fn rms(samples: &[f32]) -> f64 {
        let sum: f64 = samples.iter().map(|s| f64::from(*s).powi(2)).sum();
        (sum / samples.len() as f64).sqrt()
    }

    fn resample_in_chunks(input: &[f32], input_rate: u32, chunk: usize) -> Vec<f32> {
        let mut resampler = Resampler::new(input_rate, 16_000);
        let mut output = Vec::new();
        for block in input.chunks(chunk) {
            resampler.process(block, &mut output);
        }
        output
    }

    #[test]
    fn keeps_speech_band_tones() {
        for rate in [48_000, 44_100, 16_000] {
            let output = resample_in_chunks(&sine(rate, 1_000.0, 1.0), rate, 480);
            let expected = 16_000.0;
            assert!((output.len() as f64 - expected).abs() < 64.0, "rate {rate}: {}", output.len());
            let level = rms(&output[200..output.len() - 200]);
            assert!((level - 0.7071).abs() < 0.02, "rate {rate}: rms {level}");
        }
    }

    #[test]
    fn removes_tones_above_output_nyquist() {
        let output = resample_in_chunks(&sine(48_000, 12_000.0, 1.0), 48_000, 1024);
        assert!(rms(&output[200..]) < 0.01);
    }

    #[test]
    fn chunk_size_does_not_change_output() {
        let input = sine(48_000, 440.0, 0.5);
        let small = resample_in_chunks(&input, 48_000, 7);
        let large = resample_in_chunks(&input, 48_000, 4096);
        assert_eq!(small.len(), large.len());
        for (a, b) in small.iter().zip(&large) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
