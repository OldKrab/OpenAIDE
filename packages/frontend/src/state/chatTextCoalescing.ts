import type { ChatMessage } from "@openaide/app-shell-contracts";

export function coalesceAdjacentAgentText(items: ChatMessage[]) {
  const merged: ChatMessage[] = [];
  let index = 0;
  while (index < items.length) {
    const item = items[index];
    if (item.message.kind !== "agent_text") {
      merged.push(item);
      index += 1;
      continue;
    }

    const run = [];
    while (items[index]?.message.kind === "agent_text") {
      run.push(items[index]);
      index += 1;
    }

    if (shouldCoalesceAgentTextRun(run)) {
      merged.push(coalesceAgentTextRun(run));
    } else {
      merged.push(...run);
    }
  }
  return merged;
}

export function coalesceAdjacentThoughts(items: ChatMessage[]) {
  const merged: ChatMessage[] = [];
  let index = 0;
  while (index < items.length) {
    const item = items[index];
    if (item.message.kind !== "thought") {
      merged.push(item);
      index += 1;
      continue;
    }

    const run = [];
    while (items[index]?.message.kind === "thought") {
      run.push(items[index]);
      index += 1;
    }

    merged.push(coalesceThoughtRun(run));
  }
  return merged;
}

function coalesceThoughtRun(run: ChatMessage[]) {
  const first = run[0];
  const last = run.at(-1) ?? first;
  const text = run.map((item) => (item.message.kind === "thought" ? item.message.text : "")).join("");
  const streaming = run.some((item) => item.message.kind === "thought" && item.message.streaming === true);
  return {
    ...first,
    cursor: last.cursor,
    message: {
      ...first.message,
      text,
      streaming,
    },
  };
}

function shouldCoalesceAgentTextRun(run: ChatMessage[]) {
  if (run.length < 3) return false;
  const text = run.map((item) => (item.message.kind === "agent_text" ? item.message.text : ""));
  const tinyCount = text.filter((part) => part.trim().length <= 6).length;
  const hasChunkBoundary = text.some((part) => /^\s|\s$|[`/\\:]/.test(part));
  return tinyCount >= 2 && hasChunkBoundary;
}

function coalesceAgentTextRun(run: ChatMessage[]) {
  const first = run[0];
  const last = run.at(-1) ?? first;
  const text = run.map((item) => (item.message.kind === "agent_text" ? item.message.text : "")).join("");
  const streaming = run.some((item) => item.message.kind === "agent_text" && item.message.streaming === true);
  return {
    ...first,
    cursor: last.cursor,
    message: {
      ...first.message,
      text,
      streaming,
    },
  };
}
