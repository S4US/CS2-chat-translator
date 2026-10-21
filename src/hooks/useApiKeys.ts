import { useCallback, useEffect, useState } from "react";
import { hasApiKey } from "../tauri";
import type { KeyId } from "../types";

const KEY_IDS: KeyId[] = ["deepl", "openai-compatible", "speech-to-text"];

/** Tracks which API keys are saved in the OS keyring. */
export function useApiKeys() {
  const [hasKey, setHasKey] = useState<Record<KeyId, boolean>>({
    deepl: false,
    "openai-compatible": false,
    "speech-to-text": false,
  });

  useEffect(() => {
    void Promise.all(KEY_IDS.map((keyId) => hasApiKey(keyId).catch(() => false))).then((saved) => {
      setHasKey({ deepl: saved[0], "openai-compatible": saved[1], "speech-to-text": saved[2] });
    });
  }, []);

  const updateKey = useCallback((keyId: KeyId, saved: boolean) => {
    setHasKey((current) => ({ ...current, [keyId]: saved }));
  }, []);

  return { hasKey, updateKey };
}
