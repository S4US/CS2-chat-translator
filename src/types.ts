export type TeamScope = "ALL" | "CT" | "T";

export type ProviderId = "deepl" | "openai-compatible";

export type KeyId = ProviderId | "speech-to-text";

export type TextKind = "chat" | "voice";

export type TranslationStatus = "queued" | "translating" | "done" | "skipped" | "error";

export interface ChatMessage {
  id: string;
  team: TeamScope;
  playerName: string;
  originalText: string;
  timestamp: string;
  sourceOffset: number;
}

export interface VoiceMessage {
  id: string;
  originalText: string;
  language?: string | null;
  timestamp: string;
  durationS: number;
}

interface TranslationFields {
  translatedText?: string;
  translationStatus: TranslationStatus;
  translationError?: string;
  translationNote?: string;
  /** Already in the target language, or nothing to translate. */
  sameLanguage?: boolean;
  providerId?: ProviderId;
  targetLanguage?: string;
  fingerprint: string;
}

export type TimelineMessage =
  | (ChatMessage & TranslationFields & { source: "chat" })
  | (VoiceMessage & TranslationFields & { source: "voice" });

export type AudioDeviceKind = "input" | "monitor" | "loopback";

export interface AudioDevice {
  id: string;
  name: string;
  kind: AudioDeviceKind;
}

export interface VoiceLevel {
  rms: number;
  speech: boolean;
}

export interface VoiceSettings {
  deviceId: string | null;
  sourceLanguage: string;
  baseUrl: string;
  model: string;
  vadThreshold: number;
}

export interface OverlaySettings {
  visible: boolean;
  locked: boolean;
}

export interface OverlayState {
  messages: TimelineMessage[];
  locked: boolean;
}

export interface Settings {
  logPath?: string | null;
  targetLanguage: string;
  selectedProvider: ProviderId;
  deepl: {
    apiUrl: string;
  };
  llm: {
    baseUrl: string;
    model: string;
  };
  voice: VoiceSettings;
  overlay: OverlaySettings;
  hideSameLanguage: boolean;
  /** The user's own "term = meaning" lines for translation. */
  glossary: string;
}

export interface ProviderConfig {
  deeplApiUrl?: string;
  llmBaseUrl?: string;
  llmModel?: string;
  glossary?: string;
}

export interface Translation {
  text: string;
  sameLanguage: boolean;
}
