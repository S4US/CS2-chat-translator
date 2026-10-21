import type { ProviderId, Settings } from "./types";

/** Audio source id that captures only CS2 instead of an audio device. */
export const GAME_AUDIO_SOURCE = "app:cs2";

export const DEFAULT_SETTINGS: Settings = {
  logPath: null,
  targetLanguage: "EN",
  selectedProvider: "deepl",
  deepl: {
    apiUrl: "https://api-free.deepl.com/v2/translate",
  },
  llm: {
    baseUrl: "https://api.deepseek.com/v1",
    model: "deepseek-chat",
  },
  voice: {
    deviceId: GAME_AUDIO_SOURCE,
    sourceLanguage: "",
    baseUrl: "https://api.deepinfra.com/v1/openai",
    model: "openai/whisper-large-v3-turbo",
    vadThreshold: 0.3,
  },
  overlay: {
    visible: false,
    locked: false,
  },
  hideSameLanguage: false,
  glossary: "",
};

export const OVERLAY_WINDOW = "overlay";

export const TARGET_LANGUAGES = [
  { value: "EN", label: "English" },
  { value: "FI", label: "Finnish" },
  { value: "SV", label: "Swedish" },
  { value: "DE", label: "German" },
  { value: "FR", label: "French" },
  { value: "ES", label: "Spanish" },
  { value: "PT", label: "Portuguese" },
  { value: "PL", label: "Polish" },
  { value: "RU", label: "Russian" },
  { value: "UK", label: "Ukrainian" },
  { value: "TR", label: "Turkish" },
  { value: "ZH", label: "Chinese" },
  { value: "JA", label: "Japanese" },
  { value: "KO", label: "Korean" },
];

export const SPOKEN_LANGUAGES = [
  { value: "", label: "Detect automatically" },
  { value: "ru", label: "Russian" },
  { value: "en", label: "English" },
  { value: "uk", label: "Ukrainian" },
  { value: "pl", label: "Polish" },
  { value: "de", label: "German" },
  { value: "fr", label: "French" },
  { value: "es", label: "Spanish" },
  { value: "pt", label: "Portuguese" },
  { value: "tr", label: "Turkish" },
  { value: "fi", label: "Finnish" },
  { value: "sv", label: "Swedish" },
  { value: "zh", label: "Chinese" },
  { value: "ja", label: "Japanese" },
  { value: "ko", label: "Korean" },
];

export const PROVIDERS: Array<{ value: ProviderId; label: string }> = [
  { value: "deepl", label: "DeepL" },
  { value: "openai-compatible", label: "LLM" },
];
