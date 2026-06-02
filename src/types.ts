export type TeamScope = "ALL" | "CT" | "T";

export type ProviderId = "deepl" | "openai-compatible";

export type TranslationStatus = "queued" | "translating" | "done" | "error";

export interface ChatMessage {
  id: string;
  team: TeamScope;
  playerName: string;
  originalText: string;
  timestamp: string;
  sourceOffset: number;
}

export interface TimelineMessage extends ChatMessage {
  translatedText?: string;
  translationStatus: TranslationStatus;
  translationError?: string;
  providerId?: ProviderId;
  targetLanguage?: string;
  fingerprint: string;
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
}

export interface ProviderConfig {
  deeplApiUrl?: string;
  llmBaseUrl?: string;
  llmModel?: string;
}
