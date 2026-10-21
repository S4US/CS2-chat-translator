import { Languages, Mic, MicOff, Radio } from "lucide-react";
import type { FilteredSegments } from "../hooks/useVoice";
import type { TimelineMessage, VoiceLevel } from "../types";
import { levelPercent, statusLabel } from "../utils";

export function StatusBar({
  tailing,
  voiceOn,
  voiceWaiting,
  voiceDevice,
  voiceLevel,
  filtered,
  messages,
  providerName,
  targetLanguage,
}: {
  tailing: boolean;
  voiceOn: boolean;
  voiceWaiting: boolean;
  voiceDevice: string | null;
  voiceLevel: VoiceLevel | null;
  filtered: FilteredSegments;
  messages: TimelineMessage[];
  providerName: string;
  targetLanguage: string;
}) {
  const waiting = voiceOn && voiceWaiting;
  const voice = voiceStatus(voiceOn, waiting, Boolean(voiceLevel?.speech), voiceDevice);

  return (
    <section className="statusBand">
      <div className={tailing ? "statusPill live" : "statusPill"}>
        <Radio size={17} />
        {tailing ? "Listening" : "Idle"}
      </div>
      <div className={voice.className} title={voiceOn ? (voiceDevice ?? undefined) : undefined}>
        {voiceOn ? <Mic size={17} /> : <MicOff size={17} />}
        {voice.label}
        {voiceOn && !waiting && (
          <span className="levelMeter" aria-hidden="true">
            <span style={{ width: `${levelPercent(voiceLevel?.rms ?? 0)}%` }} />
          </span>
        )}
      </div>
      {voiceOn && filtered.count > 0 && (
        <div className="statusText" title={`Last filtered: ${filtered.lastReason}`}>
          {filtered.count} filtered
        </div>
      )}
      <div className="statusText">{statusLabel(messages)}</div>
      <div className="statusText">{providerName}</div>
      <div className="statusText">
        <Languages size={16} />
        {targetLanguage}
      </div>
    </section>
  );
}

function voiceStatus(voiceOn: boolean, waiting: boolean, speaking: boolean, device: string | null) {
  if (!voiceOn) {
    return { className: "statusPill", label: "Voice off" };
  }
  if (waiting) {
    return { className: "statusPill waiting", label: `Waiting for ${device ?? "audio"}` };
  }
  if (speaking) {
    return { className: "statusPill live speaking", label: "Hearing speech" };
  }
  return { className: "statusPill live", label: "Voice on" };
}
