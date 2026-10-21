use super::filters::{rejection_reason, TranscriptMetrics};
use super::vad::SAMPLE_RATE;
use crate::http::{self, SendError};
use reqwest::multipart;
use serde::Deserialize;

/// Whisper continues from its prompt, so a sample of real callouts biases it
/// toward CS2 vocabulary. Prompts in the wrong language can push Whisper into
/// that language, so only languages with a matching sample get one.
const RUSSIAN_PROMPT: &str = "Counter-Strike 2 voice chat in Russian. Examples of typical callouts: \"Раш Б, два на лонге!\" \"У меня AWP, прикрой смок.\" \"Один в апсе, флешка готова.\" \"Плант сделан, нужен ретейк.\" \"Сейв, инфы нет, никого не вижу.\" \"Дим на миду, заходим.\" \"Двое на сайте, страви гранату.\" Casual gaming slang and Russian obscenities are common.";
const ENGLISH_PROMPT: &str = "Counter-Strike 2 voice chat. Examples of typical callouts: \"Rush B, two on long!\" \"I've got AWP, cover the smoke.\" \"One in apps, flash ready.\" \"Bomb planted, need a retake.\" \"Save, no info.\"";

pub struct SttConfig {
    pub base_url: String,
    pub model: String,
    pub language: Option<String>,
    pub api_key: String,
}

pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
}

pub enum TranscriptionOutcome {
    Accepted(Transcript),
    Rejected(String),
}

#[derive(Debug, Deserialize)]
struct VerboseTranscription {
    text: Option<String>,
    language: Option<String>,
    #[serde(default)]
    segments: Vec<WhisperSegment>,
}

#[derive(Debug, Deserialize)]
struct WhisperSegment {
    #[serde(default)]
    no_speech_prob: f32,
    #[serde(default)]
    compression_ratio: f32,
}

pub async fn transcribe(
    config: &SttConfig,
    wav: Vec<u8>,
    duration_s: f32,
) -> Result<TranscriptionOutcome, String> {
    let url = format!(
        "{}/audio/transcriptions",
        config.base_url.trim_end_matches('/')
    );

    let response = http::send_with_retry(|| {
        http::client()
            .post(&url)
            .bearer_auth(&config.api_key)
            .multipart(build_form(config, wav.clone()))
    })
    .await
    .map_err(|err| match err {
        SendError::Status(status) => format!("Speech-to-text returned {status}"),
        SendError::Network(err) => format!("Speech-to-text request failed: {err}"),
    })?;

    let body = response
        .json::<VerboseTranscription>()
        .await
        .map_err(|err| format!("Speech-to-text returned invalid JSON: {err}"))?;
    Ok(evaluate(body, duration_s))
}

fn build_form(config: &SttConfig, wav: Vec<u8>) -> multipart::Form {
    let file = multipart::Part::bytes(wav)
        .file_name("segment.wav")
        .mime_str("audio/wav")
        .expect("audio/wav is a valid MIME type");
    let mut form = multipart::Form::new()
        .part("file", file)
        .text("model", config.model.clone())
        .text("response_format", "verbose_json")
        .text("temperature", "0");

    if let Some(language) = &config.language {
        form = form.text("language", language.clone());
        if let Some(prompt) = prompt_for(language) {
            form = form.text("prompt", prompt);
        }
    }
    form
}

fn prompt_for(language: &str) -> Option<&'static str> {
    match language {
        "ru" => Some(RUSSIAN_PROMPT),
        "en" => Some(ENGLISH_PROMPT),
        _ => None,
    }
}

fn evaluate(body: VerboseTranscription, duration_s: f32) -> TranscriptionOutcome {
    let text = body.text.unwrap_or_default().trim().to_string();
    let no_speech_prob = body
        .segments
        .iter()
        .map(|segment| segment.no_speech_prob)
        .fold(0.0, f32::max);
    let compression_ratio = body
        .segments
        .iter()
        .map(|segment| segment.compression_ratio)
        .fold(0.0, f32::max);

    let metrics = TranscriptMetrics {
        text: &text,
        no_speech_prob,
        compression_ratio,
        duration_s,
    };
    if let Some(reason) = rejection_reason(&metrics) {
        return TranscriptionOutcome::Rejected(reason);
    }
    TranscriptionOutcome::Accepted(Transcript {
        text,
        language: body.language.filter(|language| !language.is_empty()),
    })
}

/// Encodes 16 kHz mono samples as a 16-bit PCM WAV file.
pub fn encode_wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let byte_rate = SAMPLE_RATE * 2;
    let mut wav = Vec::with_capacity(44 + data_len as usize);

    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());

    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        wav.extend_from_slice(&value.to_le_bytes());
    }
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_valid_wav_header() {
        let wav = encode_wav(&[0.0, 1.0, -1.0, 2.0]);
        assert_eq!(wav.len(), 44 + 8);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 44);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
        assert_eq!(i16::from_le_bytes([wav[46], wav[47]]), i16::MAX);
        assert_eq!(i16::from_le_bytes([wav[48], wav[49]]), -i16::MAX);
        assert_eq!(i16::from_le_bytes([wav[50], wav[51]]), i16::MAX);
    }

    #[test]
    fn parses_verbose_json_and_applies_filters() {
        let accepted: VerboseTranscription = serde_json::from_str(
            r#"{"text":" Раш Б! ","language":"russian",
                "segments":[{"no_speech_prob":0.1,"compression_ratio":1.1}]}"#,
        )
        .unwrap();
        match evaluate(accepted, 1.0) {
            TranscriptionOutcome::Accepted(transcript) => {
                assert_eq!(transcript.text, "Раш Б!");
                assert_eq!(transcript.language.as_deref(), Some("russian"));
            }
            TranscriptionOutcome::Rejected(reason) => panic!("rejected: {reason}"),
        }

        let silent: VerboseTranscription = serde_json::from_str(
            r#"{"text":"Раш Б","segments":[{"no_speech_prob":0.9,"compression_ratio":1.1}]}"#,
        )
        .unwrap();
        assert!(matches!(evaluate(silent, 1.0), TranscriptionOutcome::Rejected(_)));
    }
}
