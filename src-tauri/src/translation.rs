use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{env, error::Error, fmt};
use tauri::AppHandle;

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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub deepl_api_url: Option<String>,
    pub llm_base_url: Option<String>,
    pub llm_model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeeplResponse {
    translations: Vec<DeeplTranslation>,
}

#[derive(Debug, Deserialize)]
struct DeeplTranslation {
    text: String,
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
) -> Result<String, TranslationError> {
    match provider {
        ProviderId::Deepl => translate_deepl(app, config, text, target_language).await,
        ProviderId::OpenaiCompatible => translate_llm(app, config, text, target_language).await,
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
) -> Result<String, TranslationError> {
    let api_key = get_api_key(app, ProviderId::Deepl)?;
    let url = config
        .deepl_api_url
        .unwrap_or_else(|| "https://api-free.deepl.com/v2/translate".to_string());

    let response = Client::new()
        .post(url)
        .header("Authorization", format!("DeepL-Auth-Key {api_key}"))
        .form(&[
            ("text", text.as_str()),
            ("target_lang", target_language.as_str()),
            ("preserve_formatting", "1"),
        ])
        .send()
        .await
        .map_err(|err| TranslationError::Http(err.to_string()))?;

    if !response.status().is_success() {
        return Err(TranslationError::Provider(format!(
            "DeepL returned {}",
            response.status()
        )));
    }

    let body = response
        .json::<DeeplResponse>()
        .await
        .map_err(|err| TranslationError::Http(err.to_string()))?;

    body.translations
        .into_iter()
        .next()
        .map(|translation| translation.text)
        .ok_or_else(|| TranslationError::Provider("DeepL returned no translation".to_string()))
}

async fn translate_llm(
    app: &AppHandle,
    config: ProviderConfig,
    text: String,
    target_language: String,
) -> Result<String, TranslationError> {
    let api_key = get_api_key(app, ProviderId::OpenaiCompatible)?;
    let base_url = config
        .llm_base_url
        .unwrap_or_else(|| "https://api.deepseek.com/v1".to_string())
        .trim_end_matches('/')
        .to_string();
    let model = config
        .llm_model
        .unwrap_or_else(|| "deepseek-chat".to_string());

    let system_prompt = "You translate Counter-Strike 2 chat. Preserve player names, weapon names, map callouts, profanity intensity, abbreviations, and game tone. Return only the translated chat text with no commentary.";

    let response = Client::new()
        .post(format!("{base_url}/chat/completions"))
        .bearer_auth(api_key)
        .json(&json!({
            "model": model,
            "temperature": 0.2,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": format!("Translate to {target_language}: {text}") }
            ]
        }))
        .send()
        .await
        .map_err(|err| TranslationError::Http(err.to_string()))?;

    if !response.status().is_success() {
        return Err(TranslationError::Provider(format!(
            "LLM provider returned {}",
            response.status()
        )));
    }

    let body = response
        .json::<ChatCompletionResponse>()
        .await
        .map_err(|err| TranslationError::Http(err.to_string()))?;

    body.choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content.trim().to_string())
        .filter(|translation| !translation.is_empty())
        .ok_or_else(|| TranslationError::Provider("LLM returned no translation".to_string()))
}

fn keyring_entry(
    _app: &AppHandle,
    provider: ProviderId,
) -> Result<keyring::Entry, TranslationError> {
    let account = match provider {
        ProviderId::Deepl => "deepl",
        ProviderId::OpenaiCompatible => "openai-compatible",
    };

    keyring::Entry::new("cs2-chat-translator", account)
        .map_err(|err| TranslationError::Keyring(err.to_string()))
}
