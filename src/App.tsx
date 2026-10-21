import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  Eraser,
  Loader2,
  Mic,
  MicOff,
  Pause,
  PictureInPicture2,
  Play,
  Settings as SettingsIcon,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { SettingsPanel } from "./components/SettingsPanel";
import { StatusBar } from "./components/StatusBar";
import { Timeline } from "./components/Timeline";
import { PROVIDERS } from "./constants";
import { useApiKeys } from "./hooks/useApiKeys";
import { useSettings } from "./hooks/useSettings";
import { useTranslationQueue } from "./hooks/useTranslationQueue";
import { useVoice } from "./hooks/useVoice";
import {
  onChatMessage,
  onOverlayClosed,
  onOverlayLockRequested,
  onOverlayReady,
  onTailError,
  sendOverlayState,
  setOverlay,
  startTailing,
  stopTailing,
} from "./tauri";
import type { OverlaySettings, OverlayState } from "./types";
import { errorMessage, shownWhenHiding } from "./utils";

const OVERLAY_SYNC_MESSAGES = 10;

export default function App() {
  const [tailing, setTailing] = useState(false);
  const [tailError, setTailError] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(true);
  const [appNotice, setAppNotice] = useState<string | null>(null);
  const { settings, setSettings, appliedSettings, loaded } = useSettings(setAppNotice);
  const { hasKey, updateKey } = useApiKeys();
  const { messages, addChatMessage, addVoiceMessage, retryMessage, clearMessages } = useTranslationQueue(
    appliedSettings,
    hasKey,
  );
  const voice = useVoice(addVoiceMessage);
  const visibleMessages = useMemo(
    () => (settings.hideSameLanguage ? messages.filter(shownWhenHiding) : messages),
    [messages, settings.hideSameLanguage],
  );
  const overlayState = useRef<OverlayState>({ messages: [], locked: false });

  useEffect(() => {
    if (loaded && !settings.logPath) {
      setAppNotice("Select your CS2 console.log file to start listening.");
    }
  }, [loaded]);

  useEffect(() => {
    const listeners = [
      onChatMessage(addChatMessage),
      onTailError((message) => {
        setTailError(message);
        setTailing(false);
      }),
      onOverlayReady(() => void sendOverlayState(overlayState.current)),
      onOverlayClosed(() => updateOverlay({ visible: false })),
      onOverlayLockRequested(() => updateOverlay({ locked: true })),
    ];

    return () => {
      for (const listener of listeners) {
        listener.then((unlisten) => unlisten());
      }
      stopTailing().catch(() => undefined);
    };
  }, []);

  useEffect(() => {
    if (loaded) {
      setOverlay(settings.overlay.visible, settings.overlay.locked).catch((error) =>
        setAppNotice(`Could not update the overlay: ${errorMessage(error)}`),
      );
    }
  }, [loaded, settings.overlay.visible, settings.overlay.locked]);

  useEffect(() => {
    overlayState.current = {
      messages: visibleMessages.slice(0, OVERLAY_SYNC_MESSAGES),
      locked: settings.overlay.locked,
    };
    if (settings.overlay.visible) {
      void sendOverlayState(overlayState.current).catch(() => undefined);
    }
  }, [visibleMessages, settings.overlay.visible, settings.overlay.locked]);

  const currentProviderHasKey = hasKey[settings.selectedProvider];
  const providerName = useMemo(
    () => PROVIDERS.find((provider) => provider.value === settings.selectedProvider)?.label ?? "Provider",
    [settings.selectedProvider],
  );

  function updateOverlay(changes: Partial<OverlaySettings>) {
    setSettings((current) => ({ ...current, overlay: { ...current.overlay, ...changes } }));
  }

  async function chooseLogFile() {
    const selected = await open({
      title: "Select CS2 console.log",
      multiple: false,
      filters: [{ name: "Log files", extensions: ["log"] }],
    });

    if (typeof selected === "string") {
      setSettings((current) => ({ ...current, logPath: selected }));
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
    await startTailing(settings.logPath);
    setTailing(true);
  }

  async function toggleVoice() {
    if (voice.voiceOn) {
      await voice.stop();
      return;
    }

    if (!hasKey["speech-to-text"]) {
      voice.setError("Speech-to-text needs an API key. Add it under Voice chat in settings.");
      setSettingsOpen(true);
      return;
    }

    await voice.start(settings.voice);
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
          <button className="secondaryButton" onClick={clearMessages}>
            <Eraser size={17} />
            Clear
          </button>
          <button
            className={settings.overlay.visible ? "secondaryButton active" : "secondaryButton"}
            title="Show translations above the game"
            onClick={() => updateOverlay({ visible: !settings.overlay.visible })}
          >
            <PictureInPicture2 size={17} />
            Overlay
          </button>
          <button
            className={voice.voiceOn ? "dangerButton" : "secondaryButton"}
            onClick={toggleVoice}
            disabled={voice.starting}
          >
            {voice.starting ? <Loader2 className="spin" size={17} /> : voice.voiceOn ? <MicOff size={17} /> : <Mic size={17} />}
            {voice.voiceOn ? "Stop voice" : "Voice"}
          </button>
          <button className={tailing ? "dangerButton" : "primaryButton"} onClick={toggleTailing}>
            {tailing ? <Pause size={18} /> : <Play size={18} />}
            {tailing ? "Pause" : "Listen"}
          </button>
        </div>
      </section>

      <StatusBar
        tailing={tailing}
        voiceOn={voice.voiceOn}
        voiceWaiting={voice.waiting}
        voiceDevice={voice.device}
        voiceLevel={voice.level}
        filtered={voice.filtered}
        messages={messages}
        providerName={providerName}
        targetLanguage={settings.targetLanguage}
      />

      {(tailError || voice.error || appNotice || !currentProviderHasKey) && (
        <section className="notice">
          <AlertTriangle size={18} />
          <span>
            {tailError ?? voice.error ?? appNotice ?? `${providerName} needs an API key before translations can run.`}
          </span>
        </section>
      )}

      <div className={settingsOpen ? "layout" : "layout timelineOnly"}>
        {settingsOpen && (
          <SettingsPanel
            settings={settings}
            setSettings={setSettings}
            hasKey={hasKey}
            onKeyChange={updateKey}
            audioDevices={voice.devices}
            onRefreshAudioDevices={voice.refreshDevices}
            voiceOn={voice.voiceOn}
            onChooseLogFile={chooseLogFile}
          />
        )}
        <Timeline messages={visibleMessages} onRetry={retryMessage} onPlay={voice.play} />
      </div>
    </main>
  );
}
