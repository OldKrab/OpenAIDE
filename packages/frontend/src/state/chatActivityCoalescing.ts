import type { ChatMessage } from "@openaide/app-shell-contracts";

type ActivityChatMessage = ChatMessage & { message: Extract<ChatMessage["message"], { kind: "activity" }> };
type ThoughtChatMessage = ChatMessage & {
  message: Extract<ChatMessage["message"], { kind: "agent_message" }> & { role: "thought" };
};
type ActivityRunMessage = ActivityChatMessage | ThoughtChatMessage;

/** Presents each uninterrupted run of tool activity as one ordered disclosure group. */
export function coalesceAdjacentActivities(items: ChatMessage[]) {
  const merged: ChatMessage[] = [];
  let index = 0;
  while (index < items.length) {
    const item = items[index];
    if (!isActivityRunItem(item)) {
      merged.push(item);
      index += 1;
      continue;
    }

    const run: ActivityRunMessage[] = [];
    while (items[index] && isActivityRunItem(items[index])) {
      run.push(items[index] as ActivityRunMessage);
      index += 1;
    }

    if (run.length > 1 && run.some((runItem) => runItem.message.kind === "activity")) {
      merged.push(coalesceActivityRun(run));
    } else {
      merged.push(...run);
    }
  }
  return merged;
}

function coalesceActivityRun(run: ActivityRunMessage[]): ChatMessage {
  const first = run[0];
  const last = run.at(-1) ?? first;
  const activities = run.flatMap((item) => (item.message.kind === "activity" ? [item.message] : []));
  const steps = run.flatMap((item) =>
    item.message.kind === "agent_message"
      ? [{ kind: "thought" as const, message_id: item.message.id, text: thoughtText(item.message), streaming: false }]
      : item.message.steps,
  );
  // The latest activity owns the live state; individual steps preserve failures.
  const status: ActivityChatMessage["message"]["status"] =
    activities.at(-1)?.status === "running" ? "running" : "completed";
  const firstActivity = activities[0];

  return {
    ...first,
    cursor: last.cursor,
    message_type: "activity",
    message: {
      ...(firstActivity ?? {
        kind: "activity" as const,
        id: first.message.id,
        title: "Tool activity",
        created_at: first.message.created_at,
      }),
      title: activityRunTitle(activities),
      status,
      collapsed: activities.length > 0 && activities.every((activity) => activity.collapsed),
      steps,
    },
  };
}

function activityRunTitle(activities: Extract<ChatMessage["message"], { kind: "activity" }>[]) {
  if (activities.length === 0) return "Tool activity";
  if (activities.every(isCommandActivity)) return "Commands";
  if (activities.every(isTerminalInputActivity)) return "Terminal input";
  return "Tool activity";
}

function isActivityRunItem(item: ChatMessage): item is ActivityRunMessage {
  if (item.message.kind === "agent_message") {
    return item.message.role === "thought" && item.message.parts.every((part) => part.kind === "text");
  }
  if (item.message.kind !== "activity") return false;
  // Text-only outcome Activities describe the prompt itself, not a Tool. Keeping
  // them outside the adjacent Tool run prevents limits and refusals from being
  // relabelled as generic Tool activity.
  return item.message.steps.some((step) => step.kind !== "text")
    || isTerminalInputActivity(item.message);
}

function thoughtText(message: ThoughtChatMessage["message"]) {
  return message.parts.map((part) => part.kind === "text" ? part.text : "").join("");
}

function isCommandActivity(activity: Extract<ChatMessage["message"], { kind: "activity" }>) {
  const first = activity.steps[0];
  if (first?.kind === "command") return true;
  if (first?.kind !== "tool") return false;
  const value = `${first.name} ${activity.title}`.toLowerCase();
  return first.name === "execute" || /\b(exec|command|shell|bash|terminal)\b/.test(value) || value.includes("exec_command");
}

function isTerminalInputActivity(activity: Extract<ChatMessage["message"], { kind: "activity" }>) {
  return activity.title.toLowerCase().includes("write_stdin");
}
