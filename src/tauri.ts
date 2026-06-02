import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ChatMessage, ProviderConfig, ProviderId, Settings } from "./types";

export function loadSettings() {
  return invoke<Settings>("load_settings");
}

export function saveSettings(settings: Settings) {
  return invoke<void>("save_settings", { settings });
}

export function getDefaultLogPath() {
  return invoke<string | null>("get_default_log_path");
}

export function startTailing(logPath: string) {
  return invoke<void>("start_tailing", { logPath });
}

export function stopTailing() {
  return invoke<void>("stop_tailing");
}

export function translateText(
  provider: ProviderId,
  config: ProviderConfig,
  text: string,
  targetLanguage: string,
) {
  return invoke<string>("translate_text", {
    provider,
    config,
    text,
    targetLanguage,
  });
}

export function setApiKey(provider: ProviderId, apiKey: string) {
  return invoke<void>("set_api_key", { provider, apiKey });
}

export function hasApiKey(provider: ProviderId) {
  return invoke<boolean>("has_api_key", { provider });
}

export function clearApiKey(provider: ProviderId) {
  return invoke<void>("clear_api_key", { provider });
}

export function onChatMessage(handler: (message: ChatMessage) => void) {
  return listen<ChatMessage>("chat-message", (event) => handler(event.payload));
}

export function onTailError(handler: (message: string) => void) {
  return listen<string>("tail-error", (event) => handler(event.payload));
}
