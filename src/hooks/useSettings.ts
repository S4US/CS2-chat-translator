import { useEffect, useState } from "react";
import { DEFAULT_SETTINGS } from "../constants";
import { getDefaultLogPath, loadSettings, saveSettings } from "../tauri";
import type { Settings } from "../types";
import { errorMessage } from "../utils";

const SETTINGS_DEBOUNCE_MS = 800;

/**
 * Loads settings and saves every change. `appliedSettings` follows `settings`
 * once typing pauses, so translations never use a half-typed URL or model.
 */
export function useSettings(onSaveError: (message: string) => void) {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [appliedSettings, setAppliedSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    async function load() {
      const [stored, defaultLogPath] = await Promise.all([
        loadSettings().catch(() => DEFAULT_SETTINGS),
        getDefaultLogPath().catch(() => null),
      ]);

      const nextSettings: Settings = {
        ...DEFAULT_SETTINGS,
        ...stored,
        deepl: { ...DEFAULT_SETTINGS.deepl, ...stored.deepl },
        llm: { ...DEFAULT_SETTINGS.llm, ...stored.llm },
        voice: { ...DEFAULT_SETTINGS.voice, ...stored.voice },
        overlay: { ...DEFAULT_SETTINGS.overlay, ...stored.overlay },
        logPath: stored.logPath ?? defaultLogPath,
      };
      setSettings(nextSettings);
      setAppliedSettings(nextSettings);
      setLoaded(true);
    }

    void load();
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => setAppliedSettings(settings), SETTINGS_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [settings]);

  useEffect(() => {
    if (loaded) {
      saveSettings(appliedSettings).catch((error) => onSaveError(`Could not save settings: ${errorMessage(error)}`));
    }
  }, [appliedSettings, loaded]);

  return { settings, setSettings, appliedSettings, loaded };
}
