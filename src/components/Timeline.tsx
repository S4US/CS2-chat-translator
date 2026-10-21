import { Languages, Loader2, Volume2 } from "lucide-react";
import type { TimelineMessage } from "../types";
import { formatTime } from "../utils";
import { MessageSource } from "./MessageSource";

export function Timeline({
  messages,
  onRetry,
  onPlay,
}: {
  messages: TimelineMessage[];
  onRetry: (message: TimelineMessage) => void;
  onPlay: (id: string) => void;
}) {
  return (
    <section className="timeline" aria-label="Translated chat messages">
      {messages.length === 0 ? (
        <div className="emptyState">
          <Languages size={34} />
          <h2>No new chat yet</h2>
          <p>Click Listen for text chat or Voice for voice chat. New messages appear here.</p>
        </div>
      ) : (
        messages.map((message) => (
          <article className="message" key={message.id}>
            <div className="messageMeta">
              <MessageSource message={message} />
              {message.source === "voice" && (
                <button className="playButton" title="Play recording" onClick={() => onPlay(message.id)}>
                  <Volume2 size={15} />
                </button>
              )}
              <time>{formatTime(message.timestamp)}</time>
            </div>
            <p className="original">{message.originalText || " "}</p>
            {(message.translationStatus !== "skipped" || message.translationNote) && (
              <div className={`translation ${message.translationStatus}`}>
                {message.translationStatus === "translating" && <Loader2 className="spin" size={17} />}
                {message.translationStatus === "queued" && "Queued"}
                {message.translationStatus === "translating" && "Translating"}
                {message.translationStatus === "done" && message.translatedText}
                {message.translationStatus === "skipped" && message.translationNote}
                {message.translationStatus === "error" && (
                  <>
                    <span>{message.translationError}</span>
                    <button className="linkButton" onClick={() => onRetry(message)}>
                      Retry
                    </button>
                  </>
                )}
              </div>
            )}
          </article>
        ))
      )}
    </section>
  );
}
