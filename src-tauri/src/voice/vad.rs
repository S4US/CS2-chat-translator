use ort::{environment::Environment, session::Session, value::Tensor};
use std::{collections::VecDeque, sync::OnceLock};

pub const SAMPLE_RATE: u32 = 16_000;
pub const FRAME_SAMPLES: usize = 512;

const FRAME_SECONDS: f32 = FRAME_SAMPLES as f32 / SAMPLE_RATE as f32;
const CONTEXT_SAMPLES: usize = 64;
const STATE_LEN: usize = 2 * 128;
const MODEL: &[u8] = include_bytes!("../../models/silero_vad.onnx");

/// ONNX Runtime allows one environment per process, so every voice session shares it.
static ENVIRONMENT: OnceLock<Result<Environment, String>> = OnceLock::new();

fn environment() -> Result<&'static Environment, String> {
    ENVIRONMENT
        .get_or_init(|| {
            ort::init()
                .with_name("cs2-chat-translator")
                .build()
                .map_err(|err| err.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// Gives the probability that one 512-sample frame contains speech.
pub trait SpeechModel {
    fn speech_probability(&mut self, frame: &[f32]) -> Result<f32, String>;
}

/// Silero VAD v6 running on ONNX Runtime.
pub struct SileroVad {
    session: Session,
    state: Vec<f32>,
    context: Vec<f32>,
}

impl SileroVad {
    pub fn new() -> Result<Self, String> {
        let session = Session::builder(environment()?)
            .and_then(|builder| Ok(builder.with_intra_threads(1)?))
            .and_then(|mut builder| builder.commit_from_memory(MODEL))
            .map_err(|err| format!("Could not load VAD model: {err}"))?;

        Ok(Self {
            session,
            state: vec![0.0; STATE_LEN],
            context: vec![0.0; CONTEXT_SAMPLES],
        })
    }
}

impl SpeechModel for SileroVad {
    fn speech_probability(&mut self, frame: &[f32]) -> Result<f32, String> {
        let mut input = Vec::with_capacity(CONTEXT_SAMPLES + frame.len());
        input.extend_from_slice(&self.context);
        input.extend_from_slice(frame);
        self.context.copy_from_slice(&input[input.len() - CONTEXT_SAMPLES..]);

        let input_len = input.len();
        let input = Tensor::from_array(([1usize, input_len], input));
        let state = Tensor::from_array(([2usize, 1, 128], self.state.clone()));
        let sample_rate = Tensor::from_array(((), vec![i64::from(SAMPLE_RATE)]));
        let inputs = ort::inputs![
            "input" => input.map_err(|err| err.to_string())?,
            "state" => state.map_err(|err| err.to_string())?,
            "sr" => sample_rate.map_err(|err| err.to_string())?,
        ];
        let outputs = self.session.run(inputs).map_err(|err| err.to_string())?;

        let (_, state) = outputs["stateN"]
            .try_extract_tensor::<f32>()
            .map_err(|err| err.to_string())?;
        self.state.copy_from_slice(state);

        let (_, probability) = outputs["output"]
            .try_extract_tensor::<f32>()
            .map_err(|err| err.to_string())?;
        probability
            .first()
            .copied()
            .ok_or_else(|| "VAD model returned no output".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct VadConfig {
    pub threshold: f32,
    pub start_frames: u32,
    pub end_frames: u32,
    pub min_duration_s: f32,
    pub max_duration_s: f32,
    pub preroll_s: f32,
    pub agc_target_rms: f32,
    pub agc_max_gain: f32,
    pub agc_attack_s: f32,
    pub agc_release_s: f32,
    pub agc_floor_rms: f32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            threshold: 0.3,
            start_frames: 2,
            end_frames: 8,
            min_duration_s: 0.4,
            max_duration_s: 8.0,
            preroll_s: 0.25,
            agc_target_rms: 0.05,
            agc_max_gain: 30.0,
            agc_attack_s: 0.05,
            agc_release_s: 0.5,
            agc_floor_rms: 0.0003,
        }
    }
}

/// A finished speech segment: 16 kHz mono samples after gain control.
pub struct Segment {
    pub samples: Vec<f32>,
    pub duration_s: f32,
}

/// Automatic gain control. Routed CS2 voice audio is often far quieter than
/// the levels Silero was trained on, which makes it miss real speech.
struct Agc {
    gain: f32,
    target_rms: f32,
    max_gain: f32,
    floor_rms: f32,
    attack_alpha: f32,
    release_alpha: f32,
}

impl Agc {
    fn new(config: &VadConfig) -> Self {
        Self {
            gain: 1.0,
            target_rms: config.agc_target_rms,
            max_gain: config.agc_max_gain,
            floor_rms: config.agc_floor_rms,
            attack_alpha: 1.0 - (-FRAME_SECONDS / config.agc_attack_s).exp(),
            release_alpha: 1.0 - (-FRAME_SECONDS / config.agc_release_s).exp(),
        }
    }

    fn process(&mut self, frame: &mut [f32]) {
        let level = rms(frame);
        if level > self.floor_rms {
            let target_gain = (self.target_rms / level).min(self.max_gain);
            let alpha = if target_gain < self.gain {
                self.attack_alpha
            } else {
                self.release_alpha
            };
            self.gain += (target_gain - self.gain) * alpha;
        }

        for sample in frame.iter_mut() {
            *sample = (*sample * self.gain).clamp(-1.0, 1.0);
        }
    }
}

/// Groups frames into speech segments with hysteresis, pre-roll and
/// minimum/maximum lengths, so silence and short noise never reach Whisper.
pub struct Segmenter<M: SpeechModel> {
    model: M,
    config: VadConfig,
    agc: Agc,
    speaking: bool,
    above: u32,
    below: u32,
    buffer: Vec<f32>,
    preroll: VecDeque<Vec<f32>>,
    preroll_frames: usize,
}

impl<M: SpeechModel> Segmenter<M> {
    pub fn new(model: M, config: VadConfig) -> Self {
        let preroll_frames = ((config.preroll_s / FRAME_SECONDS) as usize).max(1);
        Self {
            model,
            agc: Agc::new(&config),
            config,
            speaking: false,
            above: 0,
            below: 0,
            buffer: Vec::new(),
            preroll: VecDeque::with_capacity(preroll_frames),
            preroll_frames,
        }
    }

    pub fn is_speaking(&self) -> bool {
        self.speaking
    }

    /// Feeds one 512-sample frame. Returns a segment when speech ends.
    pub fn feed(&mut self, frame: &[f32]) -> Result<Option<Segment>, String> {
        let mut frame = frame.to_vec();
        self.agc.process(&mut frame);
        let probability = self.model.speech_probability(&frame)?;
        let is_speech = probability >= self.config.threshold;
        let mut segment = None;

        if self.speaking {
            self.buffer.extend_from_slice(&frame);
            if is_speech {
                self.below = 0;
            } else {
                self.below += 1;
            }
            let silence_ended = self.below >= self.config.end_frames;
            if silence_ended || self.duration_s() >= self.config.max_duration_s {
                segment = self.finish_segment();
            }
        } else if is_speech {
            self.above += 1;
            if self.above >= self.config.start_frames {
                self.speaking = true;
                self.above = 0;
                self.below = 0;
                self.buffer = self.preroll.iter().flatten().copied().collect();
                self.buffer.extend_from_slice(&frame);
            }
        } else {
            self.above = 0;
        }

        if self.preroll.len() == self.preroll_frames {
            self.preroll.pop_front();
        }
        self.preroll.push_back(frame);
        Ok(segment)
    }

    fn finish_segment(&mut self) -> Option<Segment> {
        let duration_s = self.duration_s();
        let samples = std::mem::take(&mut self.buffer);
        self.speaking = false;
        self.above = 0;
        self.below = 0;

        (duration_s >= self.config.min_duration_s).then_some(Segment { samples, duration_s })
    }

    fn duration_s(&self) -> f32 {
        self.buffer.len() as f32 / SAMPLE_RATE as f32
    }
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|sample| sample * sample).sum();
    (sum / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Treats any frame whose first sample is positive as speech.
    struct FakeModel;

    impl SpeechModel for FakeModel {
        fn speech_probability(&mut self, frame: &[f32]) -> Result<f32, String> {
            Ok(if frame[0] > 0.0 { 0.9 } else { 0.0 })
        }
    }

    fn feed_frames(
        segmenter: &mut Segmenter<FakeModel>,
        speech: bool,
        count: usize,
    ) -> Vec<Segment> {
        let frame = vec![if speech { 0.05 } else { 0.0 }; FRAME_SAMPLES];
        (0..count)
            .filter_map(|_| segmenter.feed(&frame).unwrap())
            .collect()
    }

    #[test]
    fn emits_segment_after_speech_ends() {
        let mut segmenter = Segmenter::new(FakeModel, VadConfig::default());
        assert!(feed_frames(&mut segmenter, false, 20).is_empty());
        assert!(feed_frames(&mut segmenter, true, 30).is_empty());
        assert!(segmenter.is_speaking());

        let segments = feed_frames(&mut segmenter, false, 8);
        assert_eq!(segments.len(), 1);
        assert!(!segmenter.is_speaking());

        // Pre-roll (7 frames) + speech frames from the start trigger onwards + trailing silence.
        let frames = segments[0].samples.len() / FRAME_SAMPLES;
        assert_eq!(frames, 7 + 29 + 8);
    }

    #[test]
    fn ignores_single_frame_blips() {
        let mut segmenter = Segmenter::new(FakeModel, VadConfig::default());
        for _ in 0..10 {
            feed_frames(&mut segmenter, true, 1);
            feed_frames(&mut segmenter, false, 1);
        }
        assert!(!segmenter.is_speaking());
    }

    #[test]
    fn drops_segments_shorter_than_minimum() {
        let config = VadConfig {
            preroll_s: 0.0,
            ..VadConfig::default()
        };
        let mut segmenter = Segmenter::new(FakeModel, config);
        feed_frames(&mut segmenter, true, 2);
        assert!(segmenter.is_speaking());
        assert!(feed_frames(&mut segmenter, false, 8).is_empty());
        assert!(!segmenter.is_speaking());
    }

    #[test]
    fn splits_long_speech_at_maximum_duration() {
        let mut segmenter = Segmenter::new(FakeModel, VadConfig::default());
        let segments = feed_frames(&mut segmenter, true, 600);
        assert!(segments.len() >= 2);
        assert!(segments.iter().all(|segment| segment.duration_s <= 8.1));
    }

    #[test]
    fn agc_boosts_quiet_audio_toward_target_level() {
        let config = VadConfig::default();
        let mut agc = Agc::new(&config);
        for _ in 0..200 {
            let mut frame = vec![0.002; FRAME_SAMPLES];
            agc.process(&mut frame);
        }
        assert!((agc.gain - 25.0).abs() < 0.5, "gain {}", agc.gain);

        let mut silent = Agc::new(&config);
        let mut frame = vec![0.0001; FRAME_SAMPLES];
        silent.process(&mut frame);
        assert_eq!(silent.gain, 1.0);
    }

    #[test]
    fn silero_scores_silence_low_across_sessions() {
        for _ in 0..2 {
            let mut model = SileroVad::new().unwrap();
            let silence = vec![0.0; FRAME_SAMPLES];
            for _ in 0..10 {
                let probability = model.speech_probability(&silence).unwrap();
                assert!(probability < 0.1, "probability {probability}");
            }
        }
    }
}
