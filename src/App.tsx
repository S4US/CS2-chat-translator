import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  Check,
  Eraser,
  FolderOpen,
  KeyRound,
  Languages,
  Loader2,
  Pause,
  Play,
  Radio,
  Save,
  Settings as SettingsIcon,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DEFAULT_SETTINGS, PROVIDERS, TARGET_LANGUAGES } from "./constants";
import {
  clearApiKey,
  getDefaultLogPath,
  hasApiKey,
  loadSettings,
  onChatMessage,
  onTailError,
  saveSettings,
  setApiKey,
  startTailing,
  stopTailing,
  translateText,
} from "./tauri";
import type { ChatMessage, ProviderId, Settings, TimelineMessage } from "./types";
import { formatTime, messageFingerprint, providerConfig, statusLabel } from "./utils";

const MAX_MESSAGES = 200;

export default function App() {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [apiKeyDraft, setApiKeyDraft] = useState("");
  const [hasKey, setHasKey] = useState<Record<ProviderId, boolean>>({
    deepl: false,
    "openai-compatible": false,
  });
  const [messages, setMessages] = useState<TimelineMessage[]>([]);
  const [tailing, setTailing] = useState(false);
  const [tailError, setTailError] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(true);
  const [appNotice, setAppNotice] = useState<string | null>(null);
  const seenFingerprints = useRef(new Set<string>());
  const generation = useRef(0);
  const queue = useRef<string[]>([]);
  const active = useRef(false);
  const messagesRef = useRef<TimelineMessage[]>([]);
  const settingsRef = useRef(settings);
  const hasKeyRef = useRef(hasKey);

  useEffect(() => {
    messagesRef.current = messages;
  }, [messages]);

  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  useEffect(() => {
    hasKeyRef.current = hasKey;
  }, [hasKey]);

  useEffect(() => {
    async function bootstrap() {
      const [loaded, defaultLogPath, deeplKey, llmKey] = await Promise.all([
        loadSettings().catch(() => DEFAULT_SETTINGS),
        getDefaultLogPath().catch(() => null),
        hasApiKey("deepl").catch(() => false),
        hasApiKey("openai-compatible").catch(() => false),
      ]);

      const nextSettings = {
        ...DEFAULT_SETTINGS,
        ...loaded,
        deepl: { ...DEFAULT_SETTINGS.deepl, ...loaded.deepl },
        llm: { ...DEFAULT_SETTINGS.llm, ...loaded.llm },
        logPath: loaded.logPath ?? defaultLogPath,
      };

      setSettings(nextSettings);
      setHasKey({ deepl: deeplKey, "openai-compatible": llmKey });
      if (!nextSettings.logPath) {
        setAppNotice("Select your CS2 console.log file to start listening.");
      }
    }

    bootstrap();
  }, []);

  useEffect(() => {
    const unlistenChat = onChatMessage(handleIncomingMessage);
    const unlistenError = onTailError((message) => {
      setTailError(message);
      setTailing(false);
    });

    return () => {
      unlistenChat.then((unlisten) => unlisten());
      unlistenError.then((unlisten) => unlisten());
      stopTailing().catch(() => undefined);
    };
  }, []);

  useEffect(() => {
    generation.current += 1;
    queue.current = [];
    active.current = false;
    setMessages((current) =>
      current.map((message) =>
        message.translationStatus === "translating" || message.translationStatus === "queued"
          ? { ...message, translationStatus: "error", translationError: "Translation settings changed" }
          : message,
      ),
    );
  }, [settings.selectedProvider, settings.targetLanguage, settings.deepl.apiUrl, settings.llm.baseUrl, settings.llm.model]);

  const currentProviderHasKey = hasKey[settings.selectedProvider];
  const providerName = useMemo(
    () => PROVIDERS.find((provider) => provider.value === settings.selectedProvider)?.label ?? "Provider",
    [settings.selectedProvider],
  );

  const handleIncomingMessage = useCallback((message: ChatMessage) => {
    const activeSettings = settingsRef.current;
    const fingerprint = messageFingerprint(message);
    if (seenFingerprints.current.has(fingerprint)) {
      return;
    }

    seenFingerprints.current.add(fingerprint);
    const timelineMessage: TimelineMessage = {
      ...message,
      fingerprint,
      translationStatus: "queued",
      providerId: activeSettings.selectedProvider,
      targetLanguage: activeSettings.targetLanguage,
    };

    messagesRef.current = [timelineMessage, ...messagesRef.current].slice(0, MAX_MESSAGES);
    setMessages(messagesRef.current);
    queue.current.push(message.id);
    void drainQueue(generation.current);
  }, []);

  async function drainQueue(queueGeneration: number) {
    if (active.current) {
      return;
    }

    active.current = true;
    try {
      while (queue.current.length > 0 && queueGeneration === generation.current) {
        const messageId = queue.current.shift();
        const message = messagesRef.current.find((item) => item.id === messageId);
        if (!message) {
          continue;
        }

        setMessages((current) =>
          current.map((item) =>
            item.id === messageId ? { ...item, translationStatus: "translating", translationError: undefined } : item,
          ),
        );

        try {
          const activeSettings = settingsRef.current;
          const activeHasKey = hasKeyRef.current;
          const activeProviderName =
            PROVIDERS.find((provider) => provider.value === activeSettings.selectedProvider)?.label ?? "Provider";

          if (!activeHasKey[activeSettings.selectedProvider]) {
            throw new Error(`Missing ${activeProviderName} API key`);
          }

          const translatedText = await translateText(
            activeSettings.selectedProvider,
            providerConfig(activeSettings),
            message.originalText,
            activeSettings.targetLanguage,
          );

          if (queueGeneration !== generation.current) {
            break;
          }

          setMessages((current) =>
            current.map((item) =>
              item.id === messageId
                ? {
                    ...item,
                    translatedText,
                    translationStatus: "done",
                    providerId: activeSettings.selectedProvider,
                    targetLanguage: activeSettings.targetLanguage,
                  }
                : item,
            ),
          );
        } catch (error) {
          const translationError = error instanceof Error ? error.message : String(error);
          setMessages((current) =>
            current.map((item) =>
              item.id === messageId ? { ...item, translationStatus: "error", translationError } : item,
            ),
          );
        }
      }
    } finally {
      active.current = false;
    }
  }

  async function chooseLogFile() {
    const selected = await open({
      title: "Select CS2 console.log",
      multiple: false,
      filters: [{ name: "Log files", extensions: ["log"] }],
    });

    if (typeof selected === "string") {
      const nextSettings = { ...settings, logPath: selected };
      setSettings(nextSettings);
      await saveSettings(nextSettings);
      setAppNotice(null);
    }
  }

  async function toggleTailing() {
    if (tailing) {
      await stopTailing();
      setTailing(false);
      return;
    }

    if (!settings.logPath) {
      await chooseLogFile();
      return;
    }

    setTailError(null);
    await saveSettings(settings);
    await startTailing(settings.logPath);
    setTailing(true);
  }

  async function saveApiKey() {
    if (!apiKeyDraft.trim()) {
      return;
    }

    await setApiKey(settings.selectedProvider, apiKeyDraft.trim());
    setApiKeyDraft("");
    setHasKey((current) => ({ ...current, [settings.selectedProvider]: true }));
  }

  async function removeApiKey() {
    await clearApiKey(settings.selectedProvider).catch(() => undefined);
    setHasKey((current) => ({ ...current, [settings.selectedProvider]: false }));
  }

  async function persistSettings(nextSettings = settings) {
    setSettings(nextSettings);
    await saveSettings(nextSettings);
    setAppNotice("Settings saved.");
  }

  function retryMessage(message: TimelineMessage) {
    setMessages((current) =>
      current.map((item) =>
        item.id === message.id
          ? { ...item, translationStatus: "queued", translationError: undefined, translatedText: undefined }
          : item,
      ),
    );
    queue.current.push(message.id);
    void drainQueue(generation.current);
  }

  function clearTimeline() {
    seenFingerprints.current.clear();
    queue.current = [];
    setMessages([]);
  }

  return (
    <main className="shell">
      <section className="topbar">
        <div>
          <h1>Chat Translator</h1>
        </div>
        <div className="actions">
          <button className="iconButton" title="Settings" onClick={() => setSettingsOpen((open) => !open)}>
            <SettingsIcon size={19} />
          </button>
          <button className="secondaryButton" onClick={clearTimeline}>
            <Eraser size={17} />
            Clear
          </button>
          <button className={tailing ? "dangerButton" : "primaryButton"} onClick={toggleTailing}>
            {tailing ? <Pause size={18} /> : <Play size={18} />}
            {tailing ? "Pause" : "Listen"}
          </button>
        </div>
      </section>

      <section className="statusBand">
        <div className={tailing ? "statusPill live" : "statusPill"}>
          <Radio size={17} />
          {tailing ? "Listening" : "Idle"}
        </div>
        <div className="statusText">{statusLabel(messages)}</div>
        <div className="statusText">{providerName}</div>
        <div className="statusText">
          <Languages size={16} />
          {settings.targetLanguage}
        </div>
      </section>

      {(tailError || appNotice || !currentProviderHasKey) && (
        <section className="notice">
          <AlertTriangle size={18} />
          <span>{tailError ?? appNotice ?? `${providerName} needs an API key before translations can run.`}</span>
        </section>
      )}

      <div className={settingsOpen ? "layout" : "layout timelineOnly"}>
        {settingsOpen && (
          <aside className="settingsPanel">
            <label>
              <span>Log file</span>
              <div className="pathPicker">
                <input value={settings.logPath ?? ""} onChange={(event) => setSettings({ ...settings, logPath: event.target.value })} />
                <button className="iconButton" title="Choose log file" onClick={chooseLogFile}>
                  <FolderOpen size={18} />
                </button>
              </div>
            </label>

            <div className="row">
              <label>
                <span>Provider</span>
                <select
                  value={settings.selectedProvider}
                  onChange={(event) => setSettings({ ...settings, selectedProvider: event.target.value as ProviderId })}
                >
                  {PROVIDERS.map((provider) => (
                    <option value={provider.value} key={provider.value}>
                      {provider.label}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                <span>Target</span>
                <select value={settings.targetLanguage} onChange={(event) => setSettings({ ...settings, targetLanguage: event.target.value })}>
                  {TARGET_LANGUAGES.map((language) => (
                    <option value={language.value} key={language.value}>
                      {language.label}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            {settings.selectedProvider === "deepl" ? (
              <label>
                <span>DeepL API URL</span>
                <input value={settings.deepl.apiUrl} onChange={(event) => setSettings({ ...settings, deepl: { apiUrl: event.target.value } })} />
              </label>
            ) : (
              <>
                <label>
                  <span>LLM base URL</span>
                  <input value={settings.llm.baseUrl} onChange={(event) => setSettings({ ...settings, llm: { ...settings.llm, baseUrl: event.target.value } })} />
                </label>
                <label>
                  <span>Model</span>
                  <input value={settings.llm.model} onChange={(event) => setSettings({ ...settings, llm: { ...settings.llm, model: event.target.value } })} />
                </label>
              </>
            )}

            <label>
              <span>API key</span>
              <div className="keyRow">
                <input
                  type="password"
                  value={apiKeyDraft}
                  placeholder={currentProviderHasKey ? "Key saved in OS keyring" : "Paste provider API key"}
                  onChange={(event) => setApiKeyDraft(event.target.value)}
                />
                <button className="iconButton" title="Save API key" onClick={saveApiKey}>
                  {currentProviderHasKey ? <Check size={18} /> : <KeyRound size={18} />}
                </button>
                <button className="iconButton dangerIcon" title="Remove API key" onClick={removeApiKey}>
                  <Trash2 size={18} />
                </button>
              </div>
            </label>

            <button className="secondaryButton fullWidth" onClick={() => persistSettings()}>
              <Save size={17} />
              Save settings
            </button>
          </aside>
        )}

        <section className="timeline" aria-label="Translated chat messages">
          {messages.length === 0 ? (
            <div className="emptyState">
              <Languages size={34} />
              <h2>No new chat yet</h2>
              <p>Start listening, then CS2 chat lines appended to console.log will appear here.</p>
            </div>
          ) : (
            messages.map((message) => (
              <article className="message" key={message.id}>
                <div className="messageMeta">
                  <span className={`teamTag ${message.team.toLowerCase()}`}>{message.team}</span>
                  <strong>{message.playerName}</strong>
                  <time>{formatTime(message.timestamp)}</time>
                </div>
                <p className="original">{message.originalText || "\u00a0"}</p>
                <div className={`translation ${message.translationStatus}`}>
                  {message.translationStatus === "translating" && <Loader2 className="spin" size={17} />}
                  {message.translationStatus === "queued" && "Queued"}
                  {message.translationStatus === "translating" && "Translating"}
                  {message.translationStatus === "done" && message.translatedText}
                  {message.translationStatus === "error" && (
                    <>
                      <span>{message.translationError}</span>
                      <button className="linkButton" onClick={() => retryMessage(message)}>
                        Retry
                      </button>
                    </>
                  )}
                </div>
              </article>
            ))
          )}
        </section>
      </div>
    </main>
  );
}
