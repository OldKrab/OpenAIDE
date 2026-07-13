import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { ChatMessage } from "@openaide/app-shell-contracts";
import type { TaskLiveTextPresentation } from "../state/store";

type TextChannel = keyof TaskLiveTextPresentation;

type Reveal = {
  messageId: string;
  text: string;
  visibleLength: number;
  settleAt?: number;
};

type Reveals = Partial<Record<TextChannel, Reveal>>;

const FRAME_MS = 16;
const CARET_SETTLE_MS = 240;

/** Reveals only text proven to have arrived live while this Task view is mounted. */
export function useLiveTextPresentation(
  taskId: string,
  items: ChatMessage[],
  signals: TaskLiveTextPresentation | undefined,
) {
  const mountedTask = useRef(taskId);
  const consumedSignals = useRef<Partial<Record<TextChannel, string>>>(signalCursors(signals));
  const previousText = useRef(textByMessageId(items));
  const [reveals, setReveals] = useState<Reveals>({});
  const revealsRef = useRef<Reveals>({});
  const taskChanged = mountedTask.current !== taskId;

  if (taskChanged) {
    mountedTask.current = taskId;
    consumedSignals.current = signalCursors(signals);
    previousText.current = textByMessageId(items);
  }

  useLayoutEffect(() => {
    if (taskChanged) {
      revealsRef.current = {};
      setReveals({});
      return;
    }

    let next = revealsRef.current;
    for (const channel of ["agent", "thought"] as const) {
      const signal = signals?.[channel];
      if (!signal || consumedSignals.current[channel] === signal.eventCursor) continue;
      const target = items.find((item) => item.message_id === signal.messageId);
      // The signal is intentionally dispatched before the authoritative snapshot so
      // unbatched React updates cannot reveal the whole suffix for one frame.
      if (!target) continue;
      const latest = latestTextMessage(items, channel);
      if (!latest) continue;
      if (latest.message_id !== signal.messageId) {
        consumedSignals.current[channel] = signal.eventCursor;
        next = withoutReveal(next, channel);
        continue;
      }
      const fullText = textOf(target);
      if (fullText === undefined) continue;
      const priorText = previousText.current.get(signal.messageId) ?? "";
      if (!fullText.startsWith(priorText) || fullText.length <= priorText.length) continue;

      consumedSignals.current[channel] = signal.eventCursor;
      const active = next[channel];
      const visibleLength = active?.messageId === signal.messageId
        ? Math.min(active.visibleLength, fullText.length)
        : priorText.length;
      next = {
        ...next,
        [channel]: {
          messageId: signal.messageId,
          text: fullText,
          visibleLength,
        },
      };
    }
    if (next !== revealsRef.current) {
      revealsRef.current = next;
      setReveals(next);
    }
    previousText.current = textByMessageId(items);
  }, [items, signals, taskChanged]);

  useEffect(() => {
    const active = Object.values(reveals);
    if (active.length === 0) return undefined;
    const now = Date.now();
    const nextDelay = active.reduce((delay, reveal) => {
      if (reveal.visibleLength < reveal.text.length) return Math.min(delay, FRAME_MS);
      return Math.min(delay, Math.max(0, (reveal.settleAt ?? now) - now));
    }, Number.POSITIVE_INFINITY);
    if (!Number.isFinite(nextDelay)) return undefined;

    const timer = window.setTimeout(() => {
      const tick = Date.now();
      const next: Reveals = {};
      for (const channel of ["agent", "thought"] as const) {
        const reveal = revealsRef.current[channel];
        if (!reveal) continue;
        if (reveal.visibleLength < reveal.text.length) {
          const remaining = reveal.text.length - reveal.visibleLength;
          const visibleLength = Math.min(
            reveal.text.length,
            reveal.visibleLength + Math.max(1, Math.ceil(remaining / 5)),
          );
          next[channel] = {
            ...reveal,
            visibleLength,
            settleAt: visibleLength === reveal.text.length ? tick + CARET_SETTLE_MS : undefined,
          };
        } else if ((reveal.settleAt ?? tick) > tick) {
          next[channel] = reveal;
        }
      }
      revealsRef.current = next;
      setReveals(next);
    }, nextDelay);
    return () => window.clearTimeout(timer);
  }, [reveals]);

  const presentedItems = useMemo(() => items.map((item) => {
    const reveal = revealForMessage(reveals, item.message_id);
    if (!reveal) return item;
    if (item.message.kind !== "agent_message") return item;
    return {
      ...item,
      message: {
        ...item.message,
        parts: visibleAgentParts(item.message.parts, reveal.visibleLength),
      },
    } as ChatMessage;
  }), [items, reveals]);

  return {
    activeMessageIds: new Set(Object.values(reveals).map((reveal) => reveal.messageId)),
    items: taskChanged ? items : presentedItems,
  };
}

function latestTextMessage(items: ChatMessage[], channel: TextChannel) {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    const item = items[index];
    if (!item) continue;
    if (item.message.kind === "agent_message" && item.message.role === channel && textOf(item) !== undefined) {
      return item;
    }
  }
  return undefined;
}

function textByMessageId(items: ChatMessage[]) {
  return new Map(items.flatMap((item) => {
    const text = textOf(item);
    return text === undefined ? [] : [[item.message_id, text] as const];
  }));
}

function textOf(item: ChatMessage) {
  if (item.message.kind !== "agent_message") return undefined;
  const text = item.message.parts
    .filter((part) => part.kind === "text")
    .map((part) => part.text)
    .join("");
  return text || undefined;
}

function visibleAgentParts(
  parts: Extract<ChatMessage["message"], { kind: "agent_message" }>["parts"],
  visibleLength: number,
) {
  let remaining = visibleLength;
  return parts.map((part) => {
    if (part.kind !== "text") return part;
    const text = part.text.slice(0, Math.max(0, remaining));
    remaining -= part.text.length;
    return { ...part, text };
  });
}

function signalCursors(signals: TaskLiveTextPresentation | undefined) {
  return {
    agent: signals?.agent?.eventCursor,
    thought: signals?.thought?.eventCursor,
  };
}

function revealForMessage(reveals: Reveals, messageId: string) {
  return Object.values(reveals).find((reveal) => reveal.messageId === messageId);
}

function withoutReveal(reveals: Reveals, channel: TextChannel) {
  if (!reveals[channel]) return reveals;
  const next = { ...reveals };
  delete next[channel];
  return next;
}
