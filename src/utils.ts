import type { ProviderConfig, Settings, TimelineMessage } from "./types";

export function messageFingerprint(message: {
  team: string;
  playerName: string;
  originalText: string;
}) {
  return `${message.team}\u0000${message.playerName}\u0000${message.originalText}`;
}

export function providerConfig(settings: Settings): ProviderConfig {
  return {
    deeplApiUrl: settings.deepl.apiUrl,
    llmBaseUrl: settings.llm.baseUrl,
    llmModel: settings.llm.model,
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
