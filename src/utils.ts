import { TARGET_LANGUAGES } from "./constants";
import type { AudioDevice, ProviderConfig, Settings, TimelineMessage } from "./types";

/** Whisper reports detected languages by English name. */
const WHISPER_LANGUAGE_CODES: Record<string, string> = {
  english: "EN",
  finnish: "FI",
  swedish: "SV",
  german: "DE",
  french: "FR",
  spanish: "ES",
  portuguese: "PT",
  polish: "PL",
  russian: "RU",
  ukrainian: "UK",
  turkish: "TR",
  chinese: "ZH",
  japanese: "JA",
  korean: "KO",
};

export function messageFingerprint(message: {
  team: string;
  playerName: string;
  originalText: string;
}) {
  return `${message.team}\u0000${message.playerName}\u0000${message.originalText}`;
}

export function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

/** Inserts a message into a newest-first timeline by its timestamp. */
export function insertByTime(messages: TimelineMessage[], message: TimelineMessage) {
  const time = Date.parse(message.timestamp);
  const index = Number.isNaN(time)
    ? 0
    : messages.findIndex((item) => Date.parse(item.timestamp) <= time);
  if (index === -1) {
    return [...messages, message];
  }
  return [...messages.slice(0, index), message, ...messages.slice(index)];
}

export function providerConfig(settings: Settings): ProviderConfig {
  return {
    deeplApiUrl: settings.deepl.apiUrl,
    llmBaseUrl: settings.llm.baseUrl,
    llmModel: settings.llm.model,
    glossary: settings.glossary,
  };
}

export function formatTime(timestamp: string) {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return "";
  }
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

export function statusLabel(messages: TimelineMessage[]) {
  const translating = messages.filter(
    (message) => message.translationStatus === "translating",
  ).length;
  const errors = messages.filter((message) => message.translationStatus === "error").length;
  if (translating > 0) {
    return `${translating} translating`;
  }
  if (errors > 0) {
    return `${errors} errors`;
  }
  return "Ready";
}

export function audioDeviceLabel(device: AudioDevice) {
  if (device.kind === "loopback") {
    return `${device.name} (output)`;
  }
  if (device.kind === "input") {
    return `${device.name} (input)`;
  }
  return device.name;
}

export function languageCode(language?: string | null) {
  const normalized = language?.trim().toLowerCase();
  if (!normalized) {
    return null;
  }
  if (normalized.length === 2) {
    return normalized.toUpperCase();
  }
  return WHISPER_LANGUAGE_CODES[normalized] ?? null;
}

/** Returns why a message needs no translation, or null when it needs one. */
export function translationSkipNote(message: TimelineMessage, targetLanguage: string) {
  if (!message.originalText.trim()) {
    return "";
  }
  if (message.source === "voice" && languageCode(message.language) === targetLanguage) {
    return sameLanguageNote(targetLanguage);
  }
  return null;
}

export function sameLanguageNote(targetLanguage: string) {
  const label = TARGET_LANGUAGES.find((language) => language.value === targetLanguage)?.label;
  return `Already in ${label ?? targetLanguage}`;
}

/**
 * With hiding on, only messages that needed a translation are shown, and only
 * once the translation is known.
 */
export function shownWhenHiding(message: TimelineMessage) {
  if (message.translationStatus === "queued" || message.translationStatus === "translating") {
    return false;
  }
  return !message.sameLanguage && message.originalText.trim() !== "";
}

export function languageLabel(language?: string | null) {
  if (!language) {
    return "Voice chat";
  }
  return language.charAt(0).toUpperCase() + language.slice(1);
}

/** Maps RMS level to a 0-100 meter width on a -60..0 dBFS scale. */
export function levelPercent(rms: number) {
  if (rms <= 0) {
    return 0;
  }
  const decibels = 20 * Math.log10(rms);
  return Math.max(0, Math.min(100, ((decibels + 60) / 60) * 100));
}
