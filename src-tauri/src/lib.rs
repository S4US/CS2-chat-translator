mod http;
mod log_tail;
mod overlay;
mod settings;
mod translation;
mod voice;

use log_tail::{default_log_path, parse_chat_line, TailController};
use settings::{Settings, VoiceSettings};
use tauri::{ipc::Response, AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_window_state::StateFlags;
use translation::{ProviderConfig, ProviderId, TextKind, Translation};
use voice::{AudioDevice, VoiceController};

pub struct AppState {
    tail: TailController,
    voice: VoiceController,
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
    kind: TextKind,
) -> Result<Translation, String> {
    translation::translate(&app, provider, config, text, target_language, kind)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn list_audio_devices() -> Result<Vec<AudioDevice>, String> {
    tauri::async_runtime::spawn_blocking(voice::list_devices)
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
async fn start_voice(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: VoiceSettings,
) -> Result<String, String> {
    let api_key = translation::get_api_key(&app, ProviderId::SpeechToText)
        .map_err(|_| "Speech-to-text needs an API key".to_string())?;
    translation::clear_voice_history();
    state.voice.start(app, settings, api_key).await
}

#[tauri::command]
fn stop_voice(state: State<AppState>) {
    state.voice.stop();
}

#[tauri::command]
fn voice_recording(state: State<AppState>, id: String) -> Result<Response, String> {
    state
        .voice
        .recording(&id)
        .map(Response::new)
        .ok_or_else(|| "This recording is no longer available".to_string())
}

#[tauri::command]
fn set_overlay(app: AppHandle, visible: bool, locked: bool) -> Result<(), String> {
    overlay::set(&app, visible, locked).map_err(|err| err.to_string())
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
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED)
                .build(),
        )
        .on_window_event(|window, event| {
            if !matches!(event, WindowEvent::Destroyed) {
                return;
            }
            let app = window.app_handle();
            if window.label() == overlay::LABEL {
                let _ = app.emit("overlay-closed", ());
            } else if let Some(overlay) = app.get_webview_window(overlay::LABEL) {
                // The overlay has no frame to close it, so it goes with the main window.
                let _ = overlay.close();
            }
        })
        .manage(AppState {
            tail: TailController::default(),
            voice: VoiceController::default(),
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
            list_audio_devices,
            start_voice,
            stop_voice,
            voice_recording,
            set_overlay,
            set_api_key,
            has_api_key,
            clear_api_key
        ])
        .run(tauri::generate_context!())
        .expect("error while running CS2 Chat Translator");
}
