import { Check, KeyRound, Trash2 } from "lucide-react";
import { useState } from "react";
import { clearApiKey, setApiKey } from "../tauri";
import type { KeyId } from "../types";
import { errorMessage } from "../utils";

export function KeyField({
  keyId,
  saved,
  placeholder,
  onChange,
}: {
  keyId: KeyId;
  saved: boolean;
  placeholder: string;
  onChange: (keyId: KeyId, saved: boolean) => void;
}) {
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function save() {
    if (!draft.trim()) {
      return;
    }

    try {
      await setApiKey(keyId, draft.trim());
      setDraft("");
      setError(null);
      onChange(keyId, true);
    } catch (saveError) {
      setError(`Could not save the key: ${errorMessage(saveError)}`);
    }
  }

  async function remove() {
    await clearApiKey(keyId).catch(() => undefined);
    setError(null);
    onChange(keyId, false);
  }

  return (
    <>
      <div className="keyRow">
        <input
          type="password"
          value={draft}
          placeholder={saved ? "Key saved in OS keyring" : placeholder}
          onChange={(event) => setDraft(event.target.value)}
        />
        <button className="iconButton" title="Save API key" onClick={save}>
          {saved ? <Check size={18} /> : <KeyRound size={18} />}
        </button>
        <button className="iconButton dangerIcon" title="Remove API key" onClick={remove}>
          <Trash2 size={18} />
        </button>
      </div>
      {error && <span className="fieldError">{error}</span>}
    </>
  );
}
