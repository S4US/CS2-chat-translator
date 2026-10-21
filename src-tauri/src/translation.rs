use crate::http::{self, SendError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::VecDeque,
    env,
    error::Error,
    fmt,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::AppHandle;

const VOICE_HISTORY_PAIRS: usize = 3;
const VOICE_HISTORY_MAX_AGE: Duration = Duration::from_secs(30);

/// Recent voice translations, sent to the LLM as examples so callouts in a
/// burst of speech are translated in a consistent style.
static VOICE_HISTORY: Mutex<VoiceHistory> = Mutex::new(VoiceHistory::new());

#[derive(Debug)]
pub enum TranslationError {
    MissingApiKey,
    Keyring(String),
    Http(String),
    Provider(String),
}

impl fmt::Display for TranslationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey => write!(formatter, "Missing API key"),
            Self::Keyring(message) => write!(formatter, "Keyring error: {message}"),
            Self::Http(message) => write!(formatter, "HTTP error: {message}"),
            Self::Provider(message) => write!(formatter, "Provider error: {message}"),
        }
    }
}

impl Error for TranslationError {}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderId {
    Deepl,
    OpenaiCompatible,
    SpeechToText,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TextKind {
    Chat,
    Voice,
}

/// The LLM replies with this when a message needs no translation.
const SAME_LANGUAGE_MARKER: &str = "[same]";
const MAX_GLOSSARY_ENTRIES: usize = 100;

const CHAT_SYSTEM_PROMPT: &str = r#"You translate Counter-Strike 2 (CS2) in-game text chat between players. Players type fast: expect typos, missing punctuation, gaming slang, and Russian written in Latin letters ("davai rush b", "suka").

OUTPUT
- Output ONLY the translation. No quotes, labels, notes, or alternatives.
- If the message is already entirely in the target language, or has nothing to translate (gg, ez, ?, +, xD, emoticons, names, numbers), output exactly [same]
- If only part of the message is in another language, translate that part and output the whole message.
- Keep player names, weapon names, map callouts, and abbreviations. Match the profanity intensity and tone exactly: never add or soften profanity."#;

const VOICE_SYSTEM_PROMPT: &str = r#"You translate live Counter-Strike 2 (CS2) voice chat. The speakers are casual gamers calling out positions, tactics, and reactions in real time. The text is a speech-to-text transcript, so it can lack punctuation or contain misheard words. Translate casually, not formally.

OUTPUT
- Output ONLY the translation. No quotes, labels, or commentary.
- ALWAYS attempt a translation. Short interjections and one-word callouts are real speech: "Стоп!" → "Stop!", "Ага." → "Yeah.", "Сюда!" → "Here!"
- If the text is already entirely in the target language, output exactly [same]
- Output a single dash (-) ONLY if the input is garbled noise with no real words.
- Keep the original length. Callouts are usually 2-6 words.

MISHEARD WORDS: if a word makes no sense in context, it is probably a CS2 term or callout that sounds similar. Translate the term the speaker meant.

PROFANITY: match the intensity exactly. NEVER add profanity that is not there. NEVER soften profanity that is.
"Чего ты?" is "What?", NOT "What the fuck?". "Блядь" is "Fuck", NOT "Damn".

NAMES: keep player names, transliterated if needed (Олег → Oleg, Катя → Katya).

EXAMPLES
"Раш Б, два на лонге!" → "Rush B, two on long!"
"У меня AWP, прикрой смок." → "I've got AWP, cover the smoke."
"Один в апсе, флешка готова." → "One in apps, flash ready."
"Чёрт, я кинусь." → "Damn, I'll peek."
"Дроп мне ствол." → "Drop me a gun.""#;

/// Shared by both prompts so slang gets its CS2 meaning instead of the
/// dictionary one, such as a flashbang instead of a USB drive.
const CS2_TERMS: &str = r#"CS2 TERMS
In every language, choose the CS2 meaning of a word over its everyday meaning. Use the standard CS2 terms players use: rush, push, peek, hold, rotate, stack, lurk, entry, execute, retake, clutch, plant, defuse, save, eco, force buy, full buy, drop, info, smoke, flash, molotov, HE, nade, utility, armor, kit, site, long, short, mid, apps, banana, connector, window, CT, T, spawn, low (low HP), one-tap, wallbang, spam, boost, bait, throw.

Russian and Ukrainian CS slang:
- флешка, флеш, слепилка → flash (a flashbang, never a USB drive)
- смок, дым, дим, дымовуха → smoke
- молик, молотов, коктейль, зажигалка, поджиг → molotov
- хаешка, хае, граната → HE grenade, nade
- раскидка → utility lineups
- броня, броник, кевлар → armor; дефуза, кусачки → defuse kit
- бомба, сишка → the bomb (C4)
- плент, плэнт → bomb site ("на плент Б" → "to B site"); плантануть, поставить бомбу → plant; дефузить → defuse
- раш → rush; пуш → push; пикнуть, кинуться → peek; холдить, держать → hold; ротейт → rotate; ретейк → retake; клатч → clutch; заход → execute
- сейв, сейвиться → save; эко → eco; форс → force buy; фулл бай, фулл закуп → full buy; закуп → buy; дроп, дропни → drop
- калаш, акашка, ак → AK; эмка → M4; авп, авик → AWP; муха → Scout (SSG 08); дигл → Deagle; тек → Tec-9; юсп → USP; зевс → Zeus; ствол → gun
- лоу → low (low HP); "один хп" → "1 HP"; минус, минус один → one down (an enemy died); фраг → kill; тапнуть, ваншот → one-tap
- кемпер, кемпит → camper, camping; нычка → hiding spot; крыса, крысит → lurker, lurking; тащер, тащит → carry, carrying; слил → threw (lost)
- прострел, прострелить → wallbang; спам → spam; фейк → fake; буст → boost
- кт, контры, конты → CTs; терры, терики → Ts; респ, спавн → spawn
- лонг, длина → long; шорт → short; мид → mid; апсы, апс → apps; банан → banana; кон, коннектор → connector; окно → window
- инфа → info; инфы нет → no info; дамаг → damage ("дал 90" → "hit him for 90")
- вх → walls (wallhack); читер → cheater; бот, нуб → bot, noob; изи → ez; гг → gg; нс → nice shot"#;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub deepl_api_url: Option<String>,
    pub llm_base_url: Option<String>,
    pub llm_model: Option<String>,
    /// The user's own "term = meaning" lines.
    #[serde(default)]
    pub glossary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub text: String,
    /// The text was already in the target language or had nothing to
    /// translate. `text` is then the original.
    pub same_language: bool,
}

#[derive(Debug, Deserialize)]
struct DeeplResponse {
    translations: Vec<DeeplTranslation>,
}

#[derive(Debug, Deserialize)]
struct DeeplTranslation {
    text: String,
    detected_source_language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionMessage,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionMessage {
    content: String,
}

pub async fn translate(
    app: &AppHandle,
    provider: ProviderId,
    config: ProviderConfig,
    text: String,
    target_language: String,
    kind: TextKind,
) -> Result<Translation, TranslationError> {
    match provider {
        ProviderId::Deepl => translate_deepl(app, config, text, target_language).await,
        ProviderId::OpenaiCompatible => {
            translate_llm(app, config, text, target_language, kind).await
        }
        ProviderId::SpeechToText => Err(TranslationError::Provider(
            "Speech-to-text is not a translation provider".to_string(),
        )),
    }
}

pub fn set_api_key(
    app: &AppHandle,
    provider: ProviderId,
    api_key: String,
) -> Result<(), TranslationError> {
    let entry = keyring_entry(app, provider)?;
    entry
        .set_password(&api_key)
        .map_err(|err| TranslationError::Keyring(err.to_string()))
}

pub fn get_api_key(app: &AppHandle, provider: ProviderId) -> Result<String, TranslationError> {
    let env_var = match provider {
        ProviderId::Deepl => "DEEPL_API_KEY",
        ProviderId::OpenaiCompatible => "OPENAI_COMPAT_API_KEY",
        ProviderId::SpeechToText => "SPEECH_TO_TEXT_API_KEY",
    };

    if let Ok(value) = env::var(env_var) {
        if !value.trim().is_empty() {
            return Ok(value);
        }
    }

    let entry = keyring_entry(app, provider)?;
    entry
        .get_password()
        .map_err(|err| TranslationError::Keyring(err.to_string()))
        .and_then(|key| {
            if key.trim().is_empty() {
                Err(TranslationError::MissingApiKey)
            } else {
                Ok(key)
            }
        })
}

pub fn clear_api_key(app: &AppHandle, provider: ProviderId) -> Result<(), TranslationError> {
    let entry = keyring_entry(app, provider)?;
    entry
        .delete_credential()
        .map_err(|err| TranslationError::Keyring(err.to_string()))
}

async fn translate_deepl(
    app: &AppHandle,
    config: ProviderConfig,
    text: String,
    target_language: String,
) -> Result<Translation, TranslationError> {
    let api_key = get_api_key(app, ProviderId::Deepl)?;
    let context = deepl_context(&glossary_entries(config.glossary.as_deref().unwrap_or("")));
    let url = config
        .deepl_api_url
        .unwrap_or_else(|| "https://api-free.deepl.com/v2/translate".to_string());

    let response = http::send_with_retry(|| {
        http::client()
            .post(&url)
            .header("Authorization", format!("DeepL-Auth-Key {api_key}"))
            .form(&[
                ("text", text.as_str()),
                ("target_lang", target_language.as_str()),
                ("preserve_formatting", "1"),
                ("context", context.as_str()),
            ])
    })
    .await
    .map_err(|err| send_error("DeepL", err))?;

    let body = response
        .json::<DeeplResponse>()
        .await
        .map_err(|err| TranslationError::Http(err.to_string()))?;

    let translation = body
        .translations
        .into_iter()
        .next()
        .ok_or_else(|| TranslationError::Provider("DeepL returned no translation".to_string()))?;

    let detected_target = translation
        .detected_source_language
        .is_some_and(|detected| base_language(&detected) == base_language(&target_language));
    Ok(if detected_target || same_text(&text, &translation.text) {
        Translation {
            text,
            same_language: true,
        }
    } else {
        Translation {
            text: translation.text,
            same_language: false,
        }
    })
}

/// DeepL takes no prompt, but context text steers word choice without being
/// translated or billed.
fn deepl_context(glossary: &[(String, String)]) -> String {
    let mut context = "Counter-Strike 2 (CS2) in-game chat between players. Gaming slang and callouts: rush, eco, save, AWP, AK, smoke, flash (flashbang), molotov, plant, defuse, site, long, short, mid, low HP.".to_string();
    if !glossary.is_empty() {
        let terms: Vec<String> = glossary
            .iter()
            .map(|(term, meaning)| format!("{term} means {meaning}"))
            .collect();
        context.push_str(&format!(" In this chat, {}.", terms.join("; ")));
    }
    context
}

async fn translate_llm(
    app: &AppHandle,
    config: ProviderConfig,
    text: String,
    target_language: String,
    kind: TextKind,
) -> Result<Translation, TranslationError> {
    let api_key = get_api_key(app, ProviderId::OpenaiCompatible)?;
    let base_url = config
        .llm_base_url
        .unwrap_or_else(|| "https://api.deepseek.com/v1".to_string())
        .trim_end_matches('/')
        .to_string();
    let model = config
        .llm_model
        .unwrap_or_else(|| "deepseek-chat".to_string());

    let glossary = glossary_entries(config.glossary.as_deref().unwrap_or(""));
    let system_prompt = system_prompt(kind, &glossary);

    let request = |source: &str| format!("Translate to {target_language}: {source}");
    let mut messages = vec![json!({ "role": "system", "content": system_prompt })];
    if kind == TextKind::Voice {
        let history = voice_history().recent(&target_language);
        for (source, translation) in history {
            messages.push(json!({ "role": "user", "content": request(&source) }));
            messages.push(json!({ "role": "assistant", "content": translation }));
        }
    }
    messages.push(json!({ "role": "user", "content": request(&text) }));

    let url = format!("{base_url}/chat/completions");
    let body = json!({
        "model": model,
        "temperature": 0.2,
        "messages": messages
    });
    let response =
        http::send_with_retry(|| http::client().post(&url).bearer_auth(&api_key).json(&body))
            .await
            .map_err(|err| send_error("LLM provider", err))?;

    let body = response
        .json::<ChatCompletionResponse>()
        .await
        .map_err(|err| TranslationError::Http(err.to_string()))?;

    let translation = body
        .choices
        .into_iter()
        .next()
        .map(|choice| match kind {
            TextKind::Chat => choice.message.content.trim().to_string(),
            TextKind::Voice => strip_wrapping_quotes(choice.message.content.trim()),
        })
        .filter(|translation| !translation.is_empty())
        .ok_or_else(|| TranslationError::Provider("LLM returned no translation".to_string()))?;

    if is_same_language_reply(&translation) || same_text(&text, &translation) {
        return Ok(Translation {
            text,
            same_language: true,
        });
    }
    if kind == TextKind::Voice {
        voice_history().remember(&target_language, &text, &translation);
    }
    Ok(Translation {
        text: translation,
        same_language: false,
    })
}

fn system_prompt(kind: TextKind, glossary: &[(String, String)]) -> String {
    let base = match kind {
        TextKind::Chat => CHAT_SYSTEM_PROMPT,
        TextKind::Voice => VOICE_SYSTEM_PROMPT,
    };
    let mut prompt = format!("{base}\n\n{CS2_TERMS}");
    if !glossary.is_empty() {
        prompt.push_str("\n\nPLAYER GLOSSARY\nThe user set these meanings. They override everything above, and the terms can also appear misspelled or misheard.\n");
        for (term, meaning) in glossary {
            prompt.push_str(&format!("- {term} → {meaning}\n"));
        }
    }
    prompt
}

/// Parses "term = meaning" lines. "->" and "→" also work as separators, and
/// lines starting with # are comments.
fn glossary_entries(glossary: &str) -> Vec<(String, String)> {
    glossary
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let line = line.replacen('→', "=", 1).replacen("->", "=", 1);
            let (term, meaning) = line.split_once('=')?;
            let (term, meaning) = (term.trim(), meaning.trim());
            (!term.is_empty() && !meaning.is_empty()).then(|| (term.to_string(), meaning.to_string()))
        })
        .take(MAX_GLOSSARY_ENTRIES)
        .collect()
}

fn is_same_language_reply(reply: &str) -> bool {
    reply
        .trim()
        .to_ascii_lowercase()
        .starts_with(SAME_LANGUAGE_MARKER)
}

/// True when a "translation" only differs from the source in case,
/// punctuation or spacing.
fn same_text(source: &str, translation: &str) -> bool {
    let normalize = |text: &str| -> String {
        text.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let source = normalize(source);
    !source.is_empty() && source == normalize(translation)
}

/// "EN-GB" and "EN" are the same language for skipping purposes.
fn base_language(code: &str) -> String {
    code.split('-').next().unwrap_or(code).trim().to_ascii_uppercase()
}

pub fn clear_voice_history() {
    voice_history().clear();
}

fn voice_history() -> std::sync::MutexGuard<'static, VoiceHistory> {
    VOICE_HISTORY.lock().expect("voice history lock poisoned")
}

struct VoiceHistory {
    target_language: String,
    pairs: VecDeque<(String, String)>,
    updated: Option<Instant>,
}

impl VoiceHistory {
    const fn new() -> Self {
        Self {
            target_language: String::new(),
            pairs: VecDeque::new(),
            updated: None,
        }
    }

    /// Returns recent pairs for this target language. Pairs older than the
    /// maximum age belong to an earlier round and are dropped.
    fn recent(&mut self, target_language: &str) -> Vec<(String, String)> {
        let stale = self
            .updated
            .is_none_or(|updated| updated.elapsed() > VOICE_HISTORY_MAX_AGE);
        if stale || self.target_language != target_language {
            self.clear();
        }
        self.pairs.iter().cloned().collect()
    }

    fn remember(&mut self, target_language: &str, source: &str, translation: &str) {
        // A dash means the model judged the transcript to be noise.
        if translation == "-" {
            return;
        }
        if self.target_language != target_language {
            self.clear();
            self.target_language = target_language.to_string();
        }
        self.pairs.push_back((source.to_string(), translation.to_string()));
        while self.pairs.len() > VOICE_HISTORY_PAIRS {
            self.pairs.pop_front();
        }
        self.updated = Some(Instant::now());
    }

    fn clear(&mut self) {
        self.pairs.clear();
        self.updated = None;
    }
}

fn send_error(provider: &str, err: SendError) -> TranslationError {
    match err {
        SendError::Status(status) => {
            TranslationError::Provider(format!("{provider} returned {status}"))
        }
        SendError::Network(err) => TranslationError::Http(err.to_string()),
    }
}

/// Models sometimes wrap a transcript translation in quotes despite the prompt.
fn strip_wrapping_quotes(text: &str) -> String {
    let unwrapped = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| text.strip_prefix('«').and_then(|rest| rest.strip_suffix('»')));
    unwrapped.unwrap_or(text).trim().to_string()
}

fn keyring_entry(
    _app: &AppHandle,
    provider: ProviderId,
) -> Result<keyring::Entry, TranslationError> {
    let account = match provider {
        ProviderId::Deepl => "deepl",
        ProviderId::OpenaiCompatible => "openai-compatible",
        ProviderId::SpeechToText => "speech-to-text",
    };

    keyring::Entry::new("cs2-chat-translator", account)
        .map_err(|err| TranslationError::Keyring(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_history_keeps_latest_pairs_for_target() {
        let mut history = VoiceHistory::new();
        for index in 0..5 {
            history.remember("EN", &format!("ru {index}"), &format!("en {index}"));
        }
        history.remember("EN", "шум", "-");

        let recent = history.recent("EN");
        assert_eq!(recent.len(), VOICE_HISTORY_PAIRS);
        assert_eq!(recent[0].0, "ru 2");
        assert_eq!(recent[2].1, "en 4");

        assert!(history.recent("FI").is_empty());
    }

    #[test]
    fn voice_history_drops_stale_pairs() {
        let mut history = VoiceHistory::new();
        history.remember("EN", "раш б", "rush B");
        let expired = VOICE_HISTORY_MAX_AGE + Duration::from_secs(1);
        history.updated = Instant::now().checked_sub(expired);
        assert!(history.recent("EN").is_empty());
    }

    #[test]
    fn parses_glossary_lines() {
        let entries = glossary_entries(
            "# my terms\nмуха = Scout\n\nлоу -> low HP\nминус → one down\nbroken line\n = no term\n",
        );
        assert_eq!(
            entries,
            vec![
                ("муха".to_string(), "Scout".to_string()),
                ("лоу".to_string(), "low HP".to_string()),
                ("минус".to_string(), "one down".to_string()),
            ]
        );
    }

    #[test]
    fn glossary_goes_last_in_the_prompt() {
        let glossary = vec![("муха".to_string(), "Scout".to_string())];
        let prompt = system_prompt(TextKind::Chat, &glossary);
        assert!(prompt.starts_with(CHAT_SYSTEM_PROMPT));
        assert!(prompt.contains(CS2_TERMS));
        assert!(prompt.trim_end().ends_with("- муха → Scout"));
        assert!(!system_prompt(TextKind::Voice, &[]).contains("PLAYER GLOSSARY"));
    }

    #[test]
    fn detects_same_language_replies() {
        assert!(is_same_language_reply("[same]"));
        assert!(is_same_language_reply("[SAME]\n"));
        assert!(!is_same_language_reply("Rush B, same as before"));

        assert!(same_text("rush b!!", "Rush B"));
        assert!(!same_text("раш б", "rush B"));
        assert!(!same_text("?", "?"));
    }

    #[test]
    fn compares_base_languages() {
        assert_eq!(base_language("EN-GB"), "EN");
        assert_eq!(base_language("pt-br"), "PT");
        assert_eq!(base_language("RU"), "RU");
    }

    #[test]
    fn strips_quotes_around_whole_translation() {
        assert_eq!(strip_wrapping_quotes("\"Rush B\""), "Rush B");
        assert_eq!(strip_wrapping_quotes("«Rush B»"), "Rush B");
        assert_eq!(strip_wrapping_quotes("He said \"go\""), "He said \"go\"");
    }
}
