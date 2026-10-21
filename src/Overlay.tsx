import { getCurrentWindow } from "@tauri-apps/api/window";
import { Lock } from "lucide-react";
import { useEffect, useState } from "react";
import { MessageSource } from "./components/MessageSource";
import { announceOverlayReady, onOverlayState, requestOverlayLock } from "./tauri";
import type { OverlayState, TimelineMessage } from "./types";

const OVERLAY_MAX_MESSAGES = 6;
const OVERLAY_MESSAGE_TTL_MS = 45_000;

/** A compact always-on-top view of the latest messages, shown over the game. */
export default function Overlay() {
  const [state, setState] = useState<OverlayState>({ messages: [], locked: false });
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    document.documentElement.classList.add("overlayRoot");
    const unlisten = onOverlayState(setState);
    void announceOverlayReady();
    const timer = window.setInterval(() => setNow(Date.now()), 1000);

    return () => {
      unlisten.then((stop) => stop());
      window.clearInterval(timer);
    };
  }, []);

  const recent = state.messages
    .filter((message) => message.originalText.trim() && now - Date.parse(message.timestamp) < OVERLAY_MESSAGE_TTL_MS)
    .slice(0, OVERLAY_MAX_MESSAGES)
    .reverse();

  return (
    <main className={state.locked ? "overlay" : "overlay unlocked"}>
      {!state.locked && (
        <header className="overlayHeader" data-tauri-drag-region>
          <span data-tauri-drag-region>Drag to move, resize from the corner</span>
          <button className="overlayButton" title="Lock and let clicks pass through" onClick={requestOverlayLock}>
            <Lock size={14} />
            Lock
          </button>
        </header>
      )}
      <div className="overlayMessages">
        {recent.length === 0 && !state.locked && <p className="overlayEmpty">Translations appear here.</p>}
        {recent.map((message) => (
          <p className="overlayLine" key={message.id}>
            <MessageSource message={message} />
            <span className={message.translationStatus === "done" ? "overlayText" : "overlayText pending"}>
              {overlayText(message)}
            </span>
          </p>
        ))}
      </div>
      {!state.locked && (
        <div className="resizeGrip" onMouseDown={() => void getCurrentWindow().startResizeDragging("SouthEast")} />
      )}
    </main>
  );
}

function overlayText(message: TimelineMessage) {
  if (message.translationStatus === "done" && message.translatedText) {
    return message.translatedText;
  }
  return message.originalText;
}
