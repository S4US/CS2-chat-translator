import type { ProviderId, Settings } from "./types";

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
};

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

export const PROVIDERS: Array<{ value: ProviderId; label: string }> = [
  { value: "deepl", label: "DeepL" },
  { value: "openai-compatible", label: "LLM" },
];
