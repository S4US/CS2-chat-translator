use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub log_path: Option<String>,
    pub target_language: String,
    pub selected_provider: String,
    pub deepl: DeeplSettings,
    pub llm: LlmSettings,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeeplSettings {
    pub api_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettings {
    pub base_url: String,
    pub model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            log_path: None,
            target_language: "EN".to_string(),
            selected_provider: "deepl".to_string(),
            deepl: DeeplSettings {
                api_url: "https://api-free.deepl.com/v2/translate".to_string(),
            },
            llm: LlmSettings {
                base_url: "https://api.deepseek.com/v1".to_string(),
                model: "deepseek-chat".to_string(),
            },
        }
    }
}

pub fn load(app: &AppHandle) -> io::Result<Settings> {
    let path = settings_path(app)?;
    if !path.exists() {
        return Ok(Settings::default());
    }

    let contents = fs::read_to_string(path)?;
    let settings = serde_json::from_str(&contents).unwrap_or_default();
    Ok(settings)
}

pub fn save(app: &AppHandle, settings: &Settings) -> io::Result<()> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = serde_json::to_string_pretty(settings)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    fs::write(path, contents)
}

fn settings_path(app: &AppHandle) -> io::Result<PathBuf> {
    let data_dir = app
        .path()
        .app_config_dir()
        .map_err(|err| io::Error::new(io::ErrorKind::NotFound, err))?;
    Ok(data_dir.join("settings.json"))
}
