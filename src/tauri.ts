import { invoke } from "@tauri-apps/api/core";
import { emit, emitTo, listen } from "@tauri-apps/api/event";
import { OVERLAY_WINDOW } from "./constants";
import type {
  AudioDevice,
  ChatMessage,
  KeyId,
  OverlayState,
  ProviderConfig,
  ProviderId,
  Settings,
  TextKind,
  Translation,
  VoiceLevel,
  VoiceMessage,
  VoiceSettings,
} from "./types";

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
  kind: TextKind,
) {
  return invoke<Translation>("translate_text", {
    provider,
    config,
    text,
    targetLanguage,
    kind,
  });
}

export function listAudioDevices() {
  return invoke<AudioDevice[]>("list_audio_devices");
}

export function startVoice(settings: VoiceSettings) {
  return invoke<string>("start_voice", { settings });
}

export function stopVoice() {
  return invoke<void>("stop_voice");
}

export function voiceRecording(id: string) {
  return invoke<ArrayBuffer>("voice_recording", { id });
}

export function setOverlay(visible: boolean, locked: boolean) {
  return invoke<void>("set_overlay", { visible, locked });
}

export function setApiKey(provider: KeyId, apiKey: string) {
  return invoke<void>("set_api_key", { provider, apiKey });
}

export function hasApiKey(provider: KeyId) {
  return invoke<boolean>("has_api_key", { provider });
}

export function clearApiKey(provider: KeyId) {
  return invoke<void>("clear_api_key", { provider });
}

export function onChatMessage(handler: (message: ChatMessage) => void) {
  return listen<ChatMessage>("chat-message", (event) => handler(event.payload));
}

export function onTailError(handler: (message: string) => void) {
  return listen<string>("tail-error", (event) => handler(event.payload));
}

export function onVoiceMessage(handler: (message: VoiceMessage) => void) {
  return listen<VoiceMessage>("voice-message", (event) => handler(event.payload));
}

export function onVoiceLevel(handler: (level: VoiceLevel) => void) {
  return listen<VoiceLevel>("voice-level", (event) => handler(event.payload));
}

export function onVoiceError(handler: (message: string) => void) {
  return listen<string>("voice-error", (event) => handler(event.payload));
}

export function onVoiceStopped(handler: (message: string) => void) {
  return listen<string>("voice-stopped", (event) => handler(event.payload));
}

/** Fires when capturing only CS2 and the game's audio appears or disappears. */
export function onVoiceWaiting(handler: (waiting: boolean) => void) {
  return listen<boolean>("voice-waiting", (event) => handler(event.payload));
}

export function onVoiceFiltered(handler: (reason: string) => void) {
  return listen<string>("voice-filtered", (event) => handler(event.payload));
}

export function sendOverlayState(state: OverlayState) {
  return emitTo(OVERLAY_WINDOW, "overlay-state", state);
}

export function onOverlayState(handler: (state: OverlayState) => void) {
  return listen<OverlayState>("overlay-state", (event) => handler(event.payload));
}

export function announceOverlayReady() {
  return emit("overlay-ready");
}

export function onOverlayReady(handler: () => void) {
  return listen("overlay-ready", () => handler());
}

export function requestOverlayLock() {
  return emit("overlay-lock-requested");
}

export function onOverlayLockRequested(handler: () => void) {
  return listen("overlay-lock-requested", () => handler());
}

export function onOverlayClosed(handler: () => void) {
  return listen("overlay-closed", () => handler());
}
