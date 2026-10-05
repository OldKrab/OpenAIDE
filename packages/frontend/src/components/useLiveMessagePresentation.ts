import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { AgentMessagePart } from "@openaide/app-shell-contracts";

type Reveal = {
  deadlineAt: number;
  text: string;
  visibleLength: number;
  settleAt?: number;
};

const FRAME_MS = 16;
// A steady trickle is shown almost as it arrives.
const MIN_PRESENTATION_LAG_MS = 96;
// Agents often deliver a whole paragraph at once. It is typed out at a
// readable pace instead of appearing as a block, but never later than this.
const MAX_PRESENTATION_LAG_MS = 800;
const COMFORTABLE_CHARS_PER_SECOND = 500;
const CARET_SETTLE_MS = 240;
// Agents also deliver text in regular batches with a pause between them. A
// batch is spread across the pause that recent arrivals predict, with a margin
// so that a slightly late batch does not show as a stall.
const ARRIVAL_GAP_MARGIN = 1.15;
const ARRIVAL_GAP_DECAY = 0.85;
// A longer silence is a new phase of the turn, not the rhythm of a stream.
const ARRIVAL_GAP_LIMIT_MS = 1_500;
// How far a frame may run ahead to end on a whole word.
const WORD_BOUNDARY_REACH = 24;

/**
 * Keeps ephemeral streaming animation local to the one Chat row it can change.
 * `urgent` shortens the reveal: something that needs the user's answer follows
 * the message, so its text must not trail behind.
 */
export function useLiveMessagePresentation({
  enabled,
  eventCursor,
  parts,
  urgent = false,
}: {
  enabled: boolean;
  eventCursor?: string;
  parts: AgentMessagePart[];
  urgent?: boolean;
}) {
  const animationAllowed = useLiveTextAnimationAllowed();
  const shouldAnimate = enabled && animationAllowed;
  const authoritativeText = textOf(parts);
  const consumedCursor = useRef<string | undefined>(undefined);
  // A stream signal can arrive before the durable row is mounted. In that case
  // the row has no earlier text baseline, so reveal the new text from its start.
  const previousText = useRef(shouldAnimate && eventCursor ? "" : authoritativeText);
  const [reveal, setReveal] = useState<Reveal | undefined>();
  const revealRef = useRef<Reveal | undefined>(undefined);
  const arrivals = useRef<{ lastAt?: number; expectedGapMs: number }>({ expectedGapMs: 0 });
  const pendingReveal = shouldAnimate
    && eventCursor
    && consumedCursor.current !== eventCursor
    && authoritativeText.startsWith(previousText.current)
    && authoritativeText.length > previousText.current.length
      ? {
          deadlineAt: revealDeadline(
            Date.now(),
            revealRef.current?.deadlineAt,
            authoritativeText.length - previousText.current.length,
            arrivals.current.expectedGapMs,
            urgent,
          ),
          text: authoritativeText,
          visibleLength: Math.min(
            revealRef.current?.visibleLength ?? previousText.current.length,
            authoritativeText.length,
          ),
        }
      : undefined;

  useLayoutEffect(() => {
    if (!shouldAnimate || !eventCursor) {
      consumedCursor.current = eventCursor;
      previousText.current = authoritativeText;
      revealRef.current = undefined;
      setReveal(undefined);
      return;
    }
    if (consumedCursor.current === eventCursor) {
      previousText.current = authoritativeText;
      return;
    }
    const priorText = previousText.current;
    if (!authoritativeText.startsWith(priorText) || authoritativeText.length <= priorText.length) {
      previousText.current = authoritativeText;
      return;
    }
    consumedCursor.current = eventCursor;
    const arrivedAt = Date.now();
    arrivals.current = observeArrival(arrivals.current, arrivedAt);
    const visibleLength = Math.min(revealRef.current?.visibleLength ?? priorText.length, authoritativeText.length);
    const next = {
      deadlineAt: revealDeadline(
        arrivedAt,
        revealRef.current?.deadlineAt,
        authoritativeText.length - priorText.length,
        arrivals.current.expectedGapMs,
        urgent,
      ),
      text: authoritativeText,
      visibleLength,
    };
    revealRef.current = next;
    setReveal(next);
    previousText.current = authoritativeText;
  }, [authoritativeText, eventCursor, shouldAnimate, urgent]);

  useLayoutEffect(() => {
    const current = revealRef.current;
    if (!urgent || !current) return;
    // The frame loop reads the ref, so the running reveal just ends sooner.
    revealRef.current = {
      ...current,
      deadlineAt: Math.min(current.deadlineAt, Date.now() + MIN_PRESENTATION_LAG_MS),
    };
  }, [urgent]);

  const presenting = reveal !== undefined;
  useEffect(() => {
    if (!presenting) return undefined;
    let cancelled = false;
    let cancelFrame: (() => void) | undefined;

    // Keep one frame loop alive for the whole presentation. Incoming chunks
    // update the target and deadline through revealRef without restarting it.
    const animate = () => {
      if (cancelled) return;
      const current = revealRef.current;
      if (!current) return;
      const tick = Date.now();
      if (current.visibleLength < current.text.length) {
        const remaining = current.text.length - current.visibleLength;
        const framesRemaining = Math.max(
          1,
          Math.ceil((current.deadlineAt - tick) / FRAME_MS),
        );
        const visibleLength = tick >= current.deadlineAt
          ? current.text.length
          : wholeWordLength(current.text, current.visibleLength + Math.ceil(remaining / framesRemaining));
        const next = {
          ...current,
          visibleLength,
          settleAt: visibleLength === current.text.length ? tick + CARET_SETTLE_MS : undefined,
        };
        revealRef.current = next;
        setReveal(next);
      } else if (current.settleAt === undefined) {
        const next = { ...current, settleAt: tick + CARET_SETTLE_MS };
        revealRef.current = next;
        setReveal(next);
      } else if (tick >= current.settleAt) {
        revealRef.current = undefined;
        setReveal(undefined);
        return;
      }
      cancelFrame = scheduleFrame(animate);
    };

    cancelFrame = scheduleFrame(animate);
    return () => {
      cancelled = true;
      cancelFrame?.();
    };
  }, [presenting]);

  const presentedReveal = pendingReveal ?? reveal;
  return useMemo(() => ({
    parts: shouldAnimate && presentedReveal ? visibleAgentParts(parts, presentedReveal.visibleLength) : parts,
    streaming: shouldAnimate && presentedReveal !== undefined,
  }), [parts, presentedReveal, shouldAnimate]);
}

function useLiveTextAnimationAllowed() {
  const [allowed, setAllowed] = useState(canAnimateLiveText);
  useEffect(() => {
    if (typeof document === "undefined") return undefined;
    const motion = typeof window.matchMedia === "function"
      ? window.matchMedia("(prefers-reduced-motion: reduce)")
      : undefined;
    const update = () => setAllowed(canAnimateLiveText());
    document.addEventListener("visibilitychange", update);
    motion?.addEventListener("change", update);
    return () => {
      document.removeEventListener("visibilitychange", update);
      motion?.removeEventListener("change", update);
    };
  }, []);
  return allowed;
}

function canAnimateLiveText() {
  if (typeof document !== "undefined" && document.visibilityState === "hidden") return false;
  return typeof window === "undefined" || typeof window.matchMedia !== "function"
    || !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * New text joins a running reveal without restarting it, so a trickle that
 * follows a paragraph cannot keep pushing the paragraph's end away. Each piece
 * of text is therefore visible within the maximum lag of its own arrival.
 */
function revealDeadline(
  now: number,
  runningDeadline: number | undefined,
  addedLength: number,
  expectedGapMs: number,
  urgent: boolean,
) {
  if (urgent) return now + MIN_PRESENTATION_LAG_MS;
  const paced = (addedLength / COMFORTABLE_CHARS_PER_SECOND) * 1_000;
  const untilNextArrival = expectedGapMs * ARRIVAL_GAP_MARGIN;
  const lag = Math.min(
    MAX_PRESENTATION_LAG_MS,
    Math.max(MIN_PRESENTATION_LAG_MS, paced, untilNextArrival),
  );
  return Math.max(runningDeadline ?? 0, now + lag);
}

/**
 * Tracks the longest recent pause between arrivals. A batch can itself arrive
 * as several quick updates, so the pause that matters is the peak, held and
 * slowly released, not the average.
 */
function observeArrival(
  arrivals: { lastAt?: number; expectedGapMs: number },
  now: number,
) {
  const gap = arrivals.lastAt === undefined ? undefined : now - arrivals.lastAt;
  if (gap === undefined || gap > ARRIVAL_GAP_LIMIT_MS) return { lastAt: now, expectedGapMs: 0 };
  return {
    lastAt: now,
    expectedGapMs: Math.max(gap, arrivals.expectedGapMs * ARRIVAL_GAP_DECAY),
  };
}

/** Ends a frame on a whole word and never inside a surrogate pair. */
function wholeWordLength(text: string, length: number) {
  if (length >= text.length) return text.length;
  const reach = Math.min(text.length, length + WORD_BOUNDARY_REACH);
  for (let index = length; index < reach; index += 1) {
    if (/\s/.test(text[index] ?? "")) return index;
  }
  const previous = text.charCodeAt(length - 1);
  return previous >= 0xd800 && previous <= 0xdbff ? length + 1 : length;
}

function scheduleFrame(callback: () => void) {
  if (typeof window.requestAnimationFrame === "function") {
    const frame = window.requestAnimationFrame(callback);
    return () => window.cancelAnimationFrame(frame);
  }
  const timer = window.setTimeout(callback, FRAME_MS);
  return () => window.clearTimeout(timer);
}

function textOf(parts: AgentMessagePart[]) {
  return parts
    .filter((part) => part.kind === "text")
    .map((part) => part.text)
    .join("");
}

function visibleAgentParts(parts: AgentMessagePart[], visibleLength: number) {
  let remaining = visibleLength;
  return parts.map((part) => {
    if (part.kind !== "text") return part;
    const text = part.text.slice(0, Math.max(0, remaining));
    remaining -= part.text.length;
    return { ...part, text };
  });
}
