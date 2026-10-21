/// Boilerplate Whisper invents from silence, music and codec noise. These are
/// dropped when they start the segment.
const HALLUCINATION_PHRASES: &[&str] = &[
    "спасибо за просмотр",
    "спасибо за внимание",
    "подписывайтесь на канал",
    "подпишись на канал",
    "субтитры сделал dimatorzok",
    "субтитры подогнал",
    "корректор:",
    "редактор субтитров",
    "продолжение следует",
    "всем пока",
    "до новых встреч",
    "thanks for watching",
    "thank you for watching",
    "please subscribe",
];

/// Real words that Whisper also produces from noise. These are dropped only
/// when they are the whole segment, so "спасибо, прикрой" and "ну давай" pass.
const HALLUCINATION_WORDS: &[&str] = &[
    "спасибо",
    "поехали",
    "понятно",
    "хорошо",
    "ладно",
    "ага",
    "да-да-да",
    "ну",
    "thank you",
    "subscribe",
    "bye",
    "you",
    "♪",
    "[music]",
    "(music)",
    "[applause]",
    "[silence]",
];

const MAX_NO_SPEECH_PROB: f32 = 0.6;
const MAX_COMPRESSION_RATIO: f32 = 2.4;
const MIN_CHARS_PER_SECOND: f32 = 1.0;
const MAX_CHARS_PER_SECOND: f32 = 35.0;

pub struct TranscriptMetrics<'a> {
    pub text: &'a str,
    pub no_speech_prob: f32,
    pub compression_ratio: f32,
    pub duration_s: f32,
}

/// Returns the reason a transcript should be dropped, or `None` to keep it.
pub fn rejection_reason(metrics: &TranscriptMetrics) -> Option<String> {
    let text = metrics.text.trim();
    if text.is_empty() {
        return Some("empty".to_string());
    }
    if metrics.no_speech_prob > MAX_NO_SPEECH_PROB {
        return Some(format!("no_speech_prob={:.2}", metrics.no_speech_prob));
    }
    if metrics.compression_ratio > MAX_COMPRESSION_RATIO {
        return Some(format!("compression_ratio={:.2}", metrics.compression_ratio));
    }
    if metrics.duration_s > 0.0 {
        let chars_per_second = text.chars().count() as f32 / metrics.duration_s;
        if !(MIN_CHARS_PER_SECOND..=MAX_CHARS_PER_SECOND).contains(&chars_per_second) {
            return Some(format!("chars/s={chars_per_second:.1}"));
        }
    }
    if is_hallucination_phrase(text) {
        return Some("known hallucination phrase".to_string());
    }
    None
}

fn is_hallucination_phrase(text: &str) -> bool {
    let normalized = text
        .trim()
        .to_lowercase()
        .trim_end_matches(['.', '!', '?', ' '])
        .to_string();
    if normalized.is_empty() {
        return true;
    }
    HALLUCINATION_WORDS.contains(&normalized.as_str())
        || HALLUCINATION_PHRASES.iter().any(|phrase| normalized.starts_with(phrase))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(text: &str, duration_s: f32) -> TranscriptMetrics<'_> {
        TranscriptMetrics {
            text,
            no_speech_prob: 0.1,
            compression_ratio: 1.2,
            duration_s,
        }
    }

    #[test]
    fn keeps_real_callouts() {
        assert_eq!(rejection_reason(&metrics("Раш Б, два на лонге!", 1.5)), None);
        assert_eq!(rejection_reason(&metrics("Спасибо, прикрой смок", 1.5)), None);
        assert_eq!(rejection_reason(&metrics("one in apps, flash ready", 1.5)), None);
        assert_eq!(rejection_reason(&metrics("Ну давай, пушим", 1.5)), None);
        assert_eq!(rejection_reason(&metrics("you guys rush B", 1.5)), None);
    }

    #[test]
    fn drops_known_hallucinations() {
        assert!(rejection_reason(&metrics("Спасибо.", 0.6)).is_some());
        assert!(rejection_reason(&metrics("Thank you for watching!", 1.5)).is_some());
        assert!(rejection_reason(&metrics("Продолжение следует...", 1.5)).is_some());
    }

    #[test]
    fn drops_low_confidence_and_looping_output() {
        let mut quiet = metrics("Раш Б", 1.0);
        quiet.no_speech_prob = 0.8;
        assert!(rejection_reason(&quiet).is_some());

        let mut looping = metrics("ха ха ха ха ха ха", 1.0);
        looping.compression_ratio = 3.0;
        assert!(rejection_reason(&looping).is_some());
    }

    #[test]
    fn drops_implausible_speaking_rates() {
        assert!(rejection_reason(&metrics("да", 4.0)).is_some());
        let long_text = "а".repeat(200);
        assert!(rejection_reason(&metrics(&long_text, 1.0)).is_some());
    }
}
