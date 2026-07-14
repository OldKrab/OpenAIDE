import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { AgentMessagePart } from "@openaide/app-shell-contracts";

type Reveal = {
  deadlineAt: number;
  text: string;
  visibleLength: number;
  settleAt?: number;
};

const FRAME_MS = 16;
const MAX_PRESENTATION_LAG_MS = 96;
const CARET_SETTLE_MS = 240;

/** Keeps ephemeral streaming animation local to the one Chat row it can change. */
export function useLiveMessagePresentation({
  enabled,
  eventCursor,
  parts,
}: {
  enabled: boolean;
  eventCursor?: string;
  parts: AgentMessagePart[];
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
  const pendingReveal = shouldAnimate
    && eventCursor
    && consumedCursor.current !== eventCursor
    && authoritativeText.startsWith(previousText.current)
    && authoritativeText.length > previousText.current.length
      ? {
          deadlineAt: Date.now() + MAX_PRESENTATION_LAG_MS,
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
    const visibleLength = Math.min(revealRef.current?.visibleLength ?? priorText.length, authoritativeText.length);
    const next = {
      deadlineAt: Date.now() + MAX_PRESENTATION_LAG_MS,
      text: authoritativeText,
      visibleLength,
    };
    revealRef.current = next;
    setReveal(next);
    previousText.current = authoritativeText;
  }, [authoritativeText, eventCursor, shouldAnimate]);

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
          : Math.min(current.text.length, current.visibleLength + Math.ceil(remaining / framesRemaining));
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
