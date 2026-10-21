import { FolderOpen, RefreshCw } from "lucide-react";
import type { Dispatch, SetStateAction } from "react";
import { GAME_AUDIO_SOURCE, PROVIDERS, SPOKEN_LANGUAGES, TARGET_LANGUAGES } from "../constants";
import type { AudioDevice, KeyId, OverlaySettings, ProviderId, Settings, VoiceSettings } from "../types";
import { audioDeviceLabel, sameLanguageNote } from "../utils";
import { KeyField } from "./KeyField";

export function SettingsPanel({
  settings,
  setSettings,
  hasKey,
  onKeyChange,
  audioDevices,
  onRefreshAudioDevices,
  voiceOn,
  onChooseLogFile,
}: {
  settings: Settings;
  setSettings: Dispatch<SetStateAction<Settings>>;
  hasKey: Record<KeyId, boolean>;
  onKeyChange: (keyId: KeyId, saved: boolean) => void;
  audioDevices: AudioDevice[];
  onRefreshAudioDevices: () => void;
  voiceOn: boolean;
  onChooseLogFile: () => void;
}) {
  function update(changes: Partial<Settings>) {
    setSettings((current) => ({ ...current, ...changes }));
  }

  function updateVoice(changes: Partial<VoiceSettings>) {
    setSettings((current) => ({ ...current, voice: { ...current.voice, ...changes } }));
  }

  function updateOverlay(changes: Partial<OverlaySettings>) {
    setSettings((current) => ({ ...current, overlay: { ...current.overlay, ...changes } }));
  }

  const selectedDeviceMissing =
    settings.voice.deviceId !== null &&
    settings.voice.deviceId !== GAME_AUDIO_SOURCE &&
    !audioDevices.some((device) => device.id === settings.voice.deviceId);

  return (
    <aside className="settingsPanel">
      <label>
        <span>Log file</span>
        <div className="pathPicker">
          <input value={settings.logPath ?? ""} onChange={(event) => update({ logPath: event.target.value })} />
          <button className="iconButton" title="Choose log file" onClick={onChooseLogFile}>
            <FolderOpen size={18} />
          </button>
        </div>
      </label>

      <div className="row">
        <label>
          <span>Provider</span>
          <select
            value={settings.selectedProvider}
            onChange={(event) => update({ selectedProvider: event.target.value as ProviderId })}
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
          <select value={settings.targetLanguage} onChange={(event) => update({ targetLanguage: event.target.value })}>
            {TARGET_LANGUAGES.map((language) => (
              <option value={language.value} key={language.value}>
                {language.label}
              </option>
            ))}
          </select>
        </label>
      </div>

      <label className="checkRow">
        <input
          type="checkbox"
          checked={settings.hideSameLanguage}
          onChange={(event) => update({ hideSameLanguage: event.target.checked })}
        />
        <span>Hide messages that need no translation</span>
      </label>
      {settings.hideSameLanguage && (
        <p className="hint">
          Hides messages marked "{sameLanguageNote(settings.targetLanguage)}". Chat messages appear once they are
          translated.
        </p>
      )}

      {settings.selectedProvider === "deepl" ? (
        <label>
          <span>DeepL API URL</span>
          <input
            value={settings.deepl.apiUrl}
            onChange={(event) => update({ deepl: { apiUrl: event.target.value } })}
          />
        </label>
      ) : (
        <>
          <label>
            <span>LLM base URL</span>
            <input
              value={settings.llm.baseUrl}
              onChange={(event) => update({ llm: { ...settings.llm, baseUrl: event.target.value } })}
            />
          </label>
          <label>
            <span>Model</span>
            <input
              value={settings.llm.model}
              onChange={(event) => update({ llm: { ...settings.llm, model: event.target.value } })}
            />
          </label>
        </>
      )}

      <label>
        <span>API key</span>
        <KeyField
          key={settings.selectedProvider}
          keyId={settings.selectedProvider}
          saved={hasKey[settings.selectedProvider]}
          placeholder="Paste provider API key"
          onChange={onKeyChange}
        />
      </label>

      <label>
        <span>CS2 terms</span>
        <textarea
          rows={4}
          value={settings.glossary}
          placeholder={"муха = Scout (SSG 08)\nлоу = low HP"}
          onChange={(event) => update({ glossary: event.target.value })}
        />
      </label>
      <p className="hint">
        One per line: word = meaning. Use it when a word gets its everyday meaning instead of its CS2 one. Common CS2
        slang is already built in.
      </p>

      <h2 className="sectionTitle">Voice chat</h2>
      {voiceOn && <p className="hint">Stop voice to change these settings.</p>}

      <label>
        <span>Audio source</span>
        <div className="pathPicker">
          <select
            value={settings.voice.deviceId ?? ""}
            disabled={voiceOn}
            onChange={(event) => updateVoice({ deviceId: event.target.value || null })}
          >
            <option value={GAME_AUDIO_SOURCE}>CS2 only</option>
            <option value="">All system audio (default output)</option>
            {selectedDeviceMissing && <option value={settings.voice.deviceId ?? ""}>Unavailable device</option>}
            {audioDevices.map((device) => (
              <option value={device.id} key={device.id}>
                {audioDeviceLabel(device)}
              </option>
            ))}
          </select>
          <button className="iconButton" title="Refresh audio devices" onClick={onRefreshAudioDevices}>
            <RefreshCw size={18} />
          </button>
        </div>
      </label>

      <label>
        <span>Spoken language</span>
        <select
          value={settings.voice.sourceLanguage}
          disabled={voiceOn}
          onChange={(event) => updateVoice({ sourceLanguage: event.target.value })}
        >
          {SPOKEN_LANGUAGES.map((language) => (
            <option value={language.value} key={language.value}>
              {language.label}
            </option>
          ))}
        </select>
      </label>

      <label>
        <span>Speech-to-text base URL</span>
        <input
          value={settings.voice.baseUrl}
          disabled={voiceOn}
          onChange={(event) => updateVoice({ baseUrl: event.target.value })}
        />
      </label>
      <label>
        <span>Speech-to-text model</span>
        <input
          value={settings.voice.model}
          disabled={voiceOn}
          onChange={(event) => updateVoice({ model: event.target.value })}
        />
      </label>

      <label>
        <span>Speech threshold: {settings.voice.vadThreshold.toFixed(2)} (lower catches quieter speech)</span>
        <input
          type="range"
          min={0.1}
          max={0.9}
          step={0.05}
          value={settings.voice.vadThreshold}
          disabled={voiceOn}
          onChange={(event) => updateVoice({ vadThreshold: Number(event.target.value) })}
        />
      </label>

      <label>
        <span>Speech-to-text API key</span>
        <KeyField
          keyId="speech-to-text"
          saved={hasKey["speech-to-text"]}
          placeholder="Paste speech-to-text API key"
          onChange={onKeyChange}
        />
      </label>

      <h2 className="sectionTitle">Overlay</h2>
      <p className="hint">
        Shows the latest translations above the game. CS2 must run in borderless windowed mode for the overlay to be
        visible.
      </p>
      <label className="checkRow">
        <input
          type="checkbox"
          checked={settings.overlay.visible}
          onChange={(event) => updateOverlay({ visible: event.target.checked })}
        />
        <span>Show overlay</span>
      </label>
      <label className="checkRow">
        <input
          type="checkbox"
          checked={settings.overlay.locked}
          onChange={(event) => updateOverlay({ locked: event.target.checked })}
        />
        <span>Lock position and let clicks pass through</span>
      </label>
    </aside>
  );
}
