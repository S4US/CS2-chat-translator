import { Mic } from "lucide-react";
import type { TimelineMessage } from "../types";
import { languageLabel } from "../utils";

/** The team or voice tag and the speaker shown before a message. */
export function MessageSource({ message }: { message: TimelineMessage }) {
  if (message.source === "chat") {
    return (
      <>
        <span className={`teamTag ${message.team.toLowerCase()}`}>{message.team}</span>
        <strong>{message.playerName}</strong>
      </>
    );
  }

  return (
    <>
      <span className="teamTag voice">
        <Mic size={13} />
        VOICE
      </span>
      <strong>{languageLabel(message.language)}</strong>
    </>
  );
}
