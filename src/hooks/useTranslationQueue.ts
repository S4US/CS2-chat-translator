import { useEffect, useRef, useState } from "react";
import { PROVIDERS } from "../constants";
import { translateText } from "../tauri";
import type { ChatMessage, KeyId, Settings, TimelineMessage, Translation, VoiceMessage } from "../types";
import {
  errorMessage,
  insertByTime,
  messageFingerprint,
  providerConfig,
  sameLanguageNote,
  translationSkipNote,
} from "../utils";

const MAX_MESSAGES = 200;
const MAX_CONCURRENT_TRANSLATIONS = 4;
const MAX_CACHED_TRANSLATIONS = 500;
const DUPLICATE_WINDOW_MS = 2000;

/**
 * Keeps the timeline and translates new messages, at most four at a time.
 * Changing provider or target language cancels queued work.
 */
export function useTranslationQueue(settings: Settings, hasKey: Record<KeyId, boolean>) {
  const [messages, setMessages] = useState<TimelineMessage[]>([]);
  const settingsRef = useRef(settings);
  const hasKeyRef = useRef(hasKey);
  const generation = useRef(0);
  const queue = useRef<TimelineMessage[]>([]);
  const activeTranslations = useRef(0);
  const translationCache = useRef(new Map<string, Translation>());
  const recentFingerprints = useRef(new Map<string, number>());

  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  useEffect(() => {
    hasKeyRef.current = hasKey;
  }, [hasKey]);

  useEffect(() => {
    generation.current += 1;
    queue.current = [];
    setMessages((current) =>
      current.map((message) =>
        message.translationStatus === "translating" || message.translationStatus === "queued"
          ? { ...message, translationStatus: "error", translationError: "Translation settings changed" }
          : message,
      ),
    );
  }, [settings.selectedProvider, settings.targetLanguage, settings.deepl.apiUrl, settings.llm.baseUrl, settings.llm.model]);

  function addChatMessage(message: ChatMessage) {
    const fingerprint = messageFingerprint(message);
    const now = Date.now();
    for (const [seenFingerprint, seenAt] of recentFingerprints.current) {
      if (now - seenAt > DUPLICATE_WINDOW_MS) {
        recentFingerprints.current.delete(seenFingerprint);
      }
    }
    if (recentFingerprints.current.has(fingerprint)) {
      return;
    }

    recentFingerprints.current.set(fingerprint, now);
    addMessage({ ...message, source: "chat", fingerprint, ...queuedFields() });
  }

  function addVoiceMessage(message: VoiceMessage) {
    addMessage({ ...message, source: "voice", fingerprint: message.id, ...queuedFields() });
  }

  function queuedFields() {
    const activeSettings = settingsRef.current;
    return {
      translationStatus: "queued" as const,
      providerId: activeSettings.selectedProvider,
      targetLanguage: activeSettings.targetLanguage,
    };
  }

  function addMessage(message: TimelineMessage) {
    const skipNote = translationSkipNote(message, settingsRef.current.targetLanguage);
    const entry: TimelineMessage =
      skipNote === null
        ? message
        : { ...message, translationStatus: "skipped", translationNote: skipNote, sameLanguage: skipNote !== "" };

    setMessages((current) => insertByTime(current, entry).slice(0, MAX_MESSAGES));
    if (skipNote === null) {
      queue.current.push(entry);
      drainQueue();
    }
  }

  function drainQueue() {
    while (activeTranslations.current < MAX_CONCURRENT_TRANSLATIONS) {
      const message = queue.current.shift();
      if (message === undefined) {
        return;
      }

      activeTranslations.current += 1;
      void translateMessage(message, generation.current).finally(() => {
        activeTranslations.current -= 1;
        drainQueue();
      });
    }
  }

  async function translateMessage(message: TimelineMessage, queueGeneration: number) {
    updateMessage(message.id, { translationStatus: "translating", translationError: undefined });

    try {
      const activeSettings = settingsRef.current;
      const cacheKey = JSON.stringify([
        activeSettings.selectedProvider,
        providerConfig(activeSettings),
        activeSettings.targetLanguage,
        message.source,
        message.originalText,
      ]);
      let translation = translationCache.current.get(cacheKey);

      if (translation === undefined) {
        if (!hasKeyRef.current[activeSettings.selectedProvider]) {
          const providerName =
            PROVIDERS.find((provider) => provider.value === activeSettings.selectedProvider)?.label ?? "Provider";
          throw new Error(`Missing ${providerName} API key`);
        }

        translation = await translateText(
          activeSettings.selectedProvider,
          providerConfig(activeSettings),
          message.originalText,
          activeSettings.targetLanguage,
          message.source,
        );
      }
      rememberTranslation(cacheKey, translation);

      if (queueGeneration === generation.current) {
        const result: Partial<TimelineMessage> = translation.sameLanguage
          ? {
              translationStatus: "skipped",
              translationNote: sameLanguageNote(activeSettings.targetLanguage),
              sameLanguage: true,
            }
          : { translatedText: translation.text, translationStatus: "done", sameLanguage: false };
        updateMessage(message.id, {
          ...result,
          providerId: activeSettings.selectedProvider,
          targetLanguage: activeSettings.targetLanguage,
        });
      }
    } catch (error) {
      if (queueGeneration === generation.current) {
        updateMessage(message.id, { translationStatus: "error", translationError: errorMessage(error) });
      }
    }
  }

  function updateMessage(id: string, changes: Partial<TimelineMessage>) {
    setMessages((current) =>
      current.map((item) => (item.id === id ? ({ ...item, ...changes } as TimelineMessage) : item)),
    );
  }

  function rememberTranslation(cacheKey: string, translation: Translation) {
    const cache = translationCache.current;
    cache.delete(cacheKey);
    cache.set(cacheKey, translation);
    if (cache.size > MAX_CACHED_TRANSLATIONS) {
      const oldest = cache.keys().next().value;
      if (oldest !== undefined) {
        cache.delete(oldest);
      }
    }
  }

  function retryMessage(message: TimelineMessage) {
    updateMessage(message.id, { translationStatus: "queued", translationError: undefined, translatedText: undefined });
    queue.current.push(message);
    drainQueue();
  }

  function clearMessages() {
    recentFingerprints.current.clear();
    queue.current = [];
    setMessages([]);
  }

  return { messages, addChatMessage, addVoiceMessage, retryMessage, clearMessages };
}
