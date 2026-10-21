mod capture;
mod filters;
mod game_audio;
mod resample;
mod transcribe;
mod vad;

pub use capture::{list_devices, AudioDevice};
pub use game_audio::SOURCE_ID as GAME_AUDIO_SOURCE_ID;

use crate::settings::VoiceSettings;
use capture::CaptureEvent;
use chrono::{DateTime, SecondsFormat, Utc};
use resample::Resampler;
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, Semaphore};
use transcribe::{SttConfig, TranscriptionOutcome};
use uuid::Uuid;
use vad::{Segment, Segmenter, SileroVad, VadConfig, FRAME_SAMPLES, SAMPLE_RATE};

const MAX_CONCURRENT_TRANSCRIPTIONS: usize = 4;
const MAX_RECORDINGS: usize = 100;
const LEVEL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceMessage {
    pub id: String,
    pub original_text: String,
    pub language: Option<String>,
    pub timestamp: String,
    pub duration_s: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VoiceLevel {
    rms: f32,
    speech: bool,
}

/// Audio of recent accepted segments, kept so the user can replay them.
type Recordings = Arc<Mutex<VecDeque<(String, Vec<u8>)>>>;

#[derive(Default)]
pub struct VoiceController {
    stop: Mutex<Option<Arc<AtomicBool>>>,
    recordings: Recordings,
}

/// State shared by the transcription tasks of one voice session.
struct Transcriber {
    app: AppHandle,
    stt: SttConfig,
    permits: Semaphore,
    recordings: Recordings,
}

impl VoiceController {
    /// Starts capture and waits until the device and VAD model are ready, so
    /// setup errors reach the caller instead of an event.
    pub async fn start(
        &self,
        app: AppHandle,
        settings: VoiceSettings,
        api_key: String,
    ) -> Result<String, String> {
        self.stop();

        let stop = Arc::new(AtomicBool::new(false));
        *self.stop.lock().expect("voice lock poisoned") = Some(stop.clone());
        let (ready_tx, ready_rx) = oneshot::channel();
        let recordings = self.recordings.clone();
        thread::spawn(move || run_pipeline(app, settings, api_key, recordings, stop, ready_tx));

        ready_rx
            .await
            .map_err(|_| "Voice capture stopped during startup".to_string())?
    }

    pub fn stop(&self) {
        if let Some(stop) = self.stop.lock().expect("voice lock poisoned").take() {
            stop.store(true, Ordering::Relaxed);
        }
    }

    /// Returns the WAV audio of a recent voice message.
    pub fn recording(&self, id: &str) -> Option<Vec<u8>> {
        self.recordings
            .lock()
            .expect("recordings lock poisoned")
            .iter()
            .find(|(recording_id, _)| recording_id == id)
            .map(|(_, wav)| wav.clone())
    }
}

fn run_pipeline(
    app: AppHandle,
    settings: VoiceSettings,
    api_key: String,
    recordings: Recordings,
    stop: Arc<AtomicBool>,
    ready: oneshot::Sender<Result<String, String>>,
) {
    let (sender, receiver) = mpsc::sync_channel::<CaptureEvent>(256);
    let setup = SileroVad::new().and_then(|model| {
        let capture = capture::open(settings.device_id.as_deref(), sender)?;
        Ok((model, capture))
    });
    let (model, capture) = match setup {
        Ok(parts) => parts,
        Err(err) => {
            let _ = ready.send(Err(err));
            return;
        }
    };
    let _ = ready.send(Ok(capture.device_name.clone()));

    let vad_config = VadConfig {
        threshold: settings.vad_threshold.clamp(0.05, 0.95),
        ..VadConfig::default()
    };
    let mut segmenter = Segmenter::new(model, vad_config);
    let mut resampler = Resampler::new(capture.sample_rate, SAMPLE_RATE);
    let mut pending: Vec<f32> = Vec::new();
    let mut level_sum = 0.0f32;
    let mut level_count = 0usize;
    let mut last_level = Instant::now();

    let transcriber = Arc::new(Transcriber {
        app: app.clone(),
        stt: SttConfig {
            base_url: settings.base_url,
            model: settings.model,
            language: Some(settings.source_language).filter(|language| !language.is_empty()),
            api_key,
        },
        permits: Semaphore::new(MAX_CONCURRENT_TRANSCRIPTIONS),
        recordings,
    });

    let mut failure = None;
    while failure.is_none() && !stop.load(Ordering::Relaxed) {
        match receiver.recv_timeout(LEVEL_INTERVAL) {
            Ok(CaptureEvent::Samples(samples)) => {
                level_sum += samples.iter().map(|sample| sample * sample).sum::<f32>();
                level_count += samples.len();
                resampler.process(&samples, &mut pending);

                let complete = pending.len() - pending.len() % FRAME_SAMPLES;
                for frame in pending[..complete].chunks(FRAME_SAMPLES) {
                    match segmenter.feed(frame) {
                        Ok(Some(segment)) => spawn_transcription(&transcriber, segment),
                        Ok(None) => {}
                        Err(err) => {
                            failure = Some(format!("Voice detection failed: {err}"));
                            break;
                        }
                    }
                }
                pending.drain(..complete);
            }
            Ok(CaptureEvent::Waiting(waiting)) => {
                let _ = app.emit("voice-waiting", waiting);
            }
            Ok(CaptureEvent::Failed(err)) => {
                failure = Some(format!("Audio capture stopped: {err}"));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                failure = Some("Audio capture stopped".to_string());
            }
        }

        if last_level.elapsed() >= LEVEL_INTERVAL {
            let rms = if level_count > 0 {
                (level_sum / level_count as f32).sqrt()
            } else {
                0.0
            };
            let _ = app.emit(
                "voice-level",
                VoiceLevel {
                    rms,
                    speech: segmenter.is_speaking(),
                },
            );
            level_sum = 0.0;
            level_count = 0;
            last_level = Instant::now();
        }
    }

    drop(capture);
    if let Some(message) = failure {
        let _ = app.emit("voice-stopped", message);
    }
}

fn spawn_transcription(transcriber: &Arc<Transcriber>, segment: Segment) {
    let transcriber = transcriber.clone();
    let speech_duration = chrono::Duration::milliseconds((segment.duration_s * 1000.0) as i64);
    let started_at = Utc::now() - speech_duration;

    tauri::async_runtime::spawn(async move {
        let Ok(_permit) = transcriber.permits.acquire().await else {
            return;
        };
        let wav = transcribe::encode_wav(&segment.samples);
        let app = &transcriber.app;

        match transcribe::transcribe(&transcriber.stt, wav.clone(), segment.duration_s).await {
            Ok(TranscriptionOutcome::Accepted(transcript)) => {
                let id = Uuid::new_v4().to_string();
                remember_recording(&transcriber.recordings, id.clone(), wav);
                let _ = app.emit(
                    "voice-message",
                    VoiceMessage {
                        id,
                        original_text: transcript.text,
                        language: transcript.language,
                        timestamp: timestamp(started_at),
                        duration_s: segment.duration_s,
                    },
                );
            }
            Ok(TranscriptionOutcome::Rejected(reason)) => {
                let _ = app.emit("voice-filtered", reason);
            }
            Err(err) => {
                let _ = app.emit("voice-error", err);
            }
        }
    });
}

fn remember_recording(recordings: &Recordings, id: String, wav: Vec<u8>) {
    let mut recordings = recordings.lock().expect("recordings lock poisoned");
    recordings.push_back((id, wav));
    while recordings.len() > MAX_RECORDINGS {
        recordings.pop_front();
    }
}

/// Millisecond UTC timestamps parse reliably in every webview.
fn timestamp(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}
