import { useEffect, useState } from "react";
import { timestampMillis } from "./taskSurfaceHelpers";

/** Keeps clock ticks inside the live indicator so the surrounding Chat timeline stays stable. */
export function useElapsedSeconds(startedAt?: string) {
  const startedAtMs = startedAt ? timestampMillis(startedAt) : Number.NaN;
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    setNow(Date.now());
    if (Number.isNaN(startedAtMs)) return undefined;
    const timer = globalThis.setInterval(() => setNow(Date.now()), 1_000);
    return () => globalThis.clearInterval(timer);
  }, [startedAtMs]);
  if (Number.isNaN(startedAtMs)) return undefined;
  return Math.max(0, Math.floor((now - startedAtMs) / 1_000));
}

export function formatElapsedDuration(totalSeconds: number) {
  const hours = Math.floor(totalSeconds / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) return `${hours}h ${String(minutes).padStart(2, "0")}m`;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

export function elapsedDurationLabel(totalSeconds: number) {
  const hours = Math.floor(totalSeconds / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  return [
    hours ? `${hours} hour${hours === 1 ? "" : "s"}` : undefined,
    minutes ? `${minutes} minute${minutes === 1 ? "" : "s"}` : undefined,
    `${seconds} second${seconds === 1 ? "" : "s"}`,
  ].filter(Boolean).join(" ");
}

/** Elapsed time is hidden for the first seconds so short work does not flash a clock. */
export const ELAPSED_VISIBLE_AFTER_SECONDS = 5;
