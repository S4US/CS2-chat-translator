import { useEffect, useState } from "react";
import {
  listAudioDevices,
  onVoiceError,
  onVoiceFiltered,
  onVoiceLevel,
  onVoiceMessage,
  onVoiceStopped,
  onVoiceWaiting,
  startVoice,
  stopVoice,
  voiceRecording,
} from "../tauri";
import type { AudioDevice, VoiceLevel, VoiceMessage, VoiceSettings } from "../types";
import { errorMessage } from "../utils";

export interface FilteredSegments {
  count: number;
  lastReason: string | null;
}

const NO_FILTERED: FilteredSegments = { count: 0, lastReason: null };

/** Runs voice capture and tracks its level, errors and filtered segments. */
export function useVoice(onMessage: (message: VoiceMessage) => void) {
  const [voiceOn, setVoiceOn] = useState(false);
  const [starting, setStarting] = useState(false);
  const [device, setDevice] = useState<string | null>(null);
  const [level, setLevel] = useState<VoiceLevel | null>(null);
  const [waiting, setWaiting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [filtered, setFiltered] = useState<FilteredSegments>(NO_FILTERED);
  const [devices, setDevices] = useState<AudioDevice[]>([]);

  useEffect(() => {
    void refreshDevices();
    const listeners = [
      onVoiceMessage((message) => {
        setError(null);
        onMessage(message);
      }),
      onVoiceLevel(setLevel),
      onVoiceError(setError),
      onVoiceWaiting(setWaiting),
      onVoiceFiltered((reason) => setFiltered((current) => ({ count: current.count + 1, lastReason: reason }))),
      onVoiceStopped((message) => {
        setError(message);
        setVoiceOn(false);
        setLevel(null);
        setWaiting(false);
      }),
    ];

    return () => {
      for (const listener of listeners) {
        listener.then((unlisten) => unlisten());
      }
      stopVoice().catch(() => undefined);
    };
  }, []);

  async function refreshDevices() {
    setDevices(await listAudioDevices().catch(() => []));
  }

  async function start(settings: VoiceSettings) {
    setError(null);
    setFiltered(NO_FILTERED);
    setWaiting(false);
    setStarting(true);
    try {
      setDevice(await startVoice(settings));
      setVoiceOn(true);
    } catch (startError) {
      setError(errorMessage(startError));
    } finally {
      setStarting(false);
    }
  }

  async function stop() {
    await stopVoice();
    setVoiceOn(false);
    setLevel(null);
    setWaiting(false);
  }

  async function play(id: string) {
    try {
      const wav = await voiceRecording(id);
      const url = URL.createObjectURL(new Blob([wav], { type: "audio/wav" }));
      const audio = new Audio(url);
      audio.addEventListener("ended", () => URL.revokeObjectURL(url));
      await audio.play();
    } catch (playError) {
      setError(`Could not play the recording: ${errorMessage(playError)}`);
    }
  }

  return { voiceOn, starting, waiting, device, level, error, setError, filtered, devices, refreshDevices, start, stop, play };
}
