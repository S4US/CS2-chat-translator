mod log_tail;
mod settings;
mod translation;

use log_tail::{default_log_path, parse_chat_line, TailController};
use settings::Settings;
use tauri::{AppHandle, Emitter, State};
use translation::{ProviderConfig, ProviderId};

pub struct AppState {
    tail: TailController,
}

#[tauri::command]
fn get_default_log_path() -> Option<String> {
    default_log_path()
        .filter(|path| path.exists())
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
fn parse_chat_preview(line: String) -> Option<log_tail::ChatMessage> {
    parse_chat_line(&line, 0)
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<Settings, String> {
    settings::load(&app).map_err(|err| err.to_string())
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    settings::save(&app, &settings).map_err(|err| err.to_string())
}

#[tauri::command]
fn start_tailing(app: AppHandle, state: State<AppState>, log_path: String) -> Result<(), String> {
    state
        .tail
        .start(app, log_path)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn stop_tailing(state: State<AppState>) {
    state.tail.stop();
}

#[tauri::command]
async fn translate_text(
    app: AppHandle,
    provider: ProviderId,
    config: ProviderConfig,
    text: String,
    target_language: String,
) -> Result<String, String> {
    translation::translate(&app, provider, config, text, target_language)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn set_api_key(app: AppHandle, provider: ProviderId, api_key: String) -> Result<(), String> {
    translation::set_api_key(&app, provider, api_key).map_err(|err| err.to_string())
}

#[tauri::command]
fn has_api_key(app: AppHandle, provider: ProviderId) -> bool {
    translation::get_api_key(&app, provider).is_ok()
}

#[tauri::command]
fn clear_api_key(app: AppHandle, provider: ProviderId) -> Result<(), String> {
    translation::clear_api_key(&app, provider).map_err(|err| err.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            tail: TailController::default(),
        })
        .setup(|app| {
            let settings = settings::load(app.handle()).unwrap_or_default();
            app.handle()
                .emit("settings-loaded", settings)
                .map_err(|err| err.to_string())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_default_log_path,
            parse_chat_preview,
            load_settings,
            save_settings,
            start_tailing,
            stop_tailing,
            translate_text,
            set_api_key,
            has_api_key,
            clear_api_key
        ])
        .run(tauri::generate_context!())
        .expect("error while running CS2 Chat Translator");
}
