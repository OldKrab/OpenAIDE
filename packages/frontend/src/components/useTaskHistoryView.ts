import { useMemo, useRef } from "react";
import type { ChatItem } from "@openaide/app-server-client";
import type { ChatMessage, TaskSnapshot } from "@openaide/app-shell-contracts";

import { mapProtocolChatItem } from "../state/appServerProtocolChatMapping";
import { presentedChatItems, renderedChat } from "../state/chatPaging";
import type { AppState } from "../state/store";
import type { useSubagentSessions } from "./useSubagentSessions";

/**
 * The chat and plan a Task View renders. They follow the selected subagent history when one is
 * selected, and fall back to the Task's own history; the App Server owns both projections, so
 * this only maps them into the render shape.
 */
export function useTaskHistoryView({
  chatPageState,
  snapshot,
  subagents,
}: {
  chatPageState: AppState["chatPages"][string] | undefined;
  snapshot: TaskSnapshot;
  subagents: ReturnType<typeof useSubagentSessions>;
}) {
  const mainChat = useMemo(() => renderedChat(snapshot, chatPageState), [chatPageState, snapshot]);
  const childItems = useRef<Map<string, MappedChildItem>>(new Map());
  const childChat = useMemo(() => {
    if (!subagents.selected) return undefined;
    const history = subagents.history;
    const mapped = stableChildItems(childItems.current, history?.chat.items ?? [], snapshot.task.updated_at);
    childItems.current = mapped.cache;
    return {
      items: presentedChatItems(mapped.items),
      hasBefore: history?.chat.hasMoreBefore === true,
      beforeCursor: history?.chat.startCursor ?? undefined,
      pending: history === undefined,
      error: history?.availability === "unavailable" ? "This subagent history is unavailable." : undefined,
    };
  }, [snapshot.task.updated_at, subagents.history, subagents.selected]);
  const visiblePlan = useMemo(() => {
    if (!subagents.selected) return snapshot.current_plan;
    const plan = subagents.history?.currentPlan;
    return plan ? {
      entries: plan.entries.map((entry) => ({
        content: entry.content,
        priority: entry.priority,
        status: entry.status === "inProgress" ? "in_progress" as const : entry.status,
      })),
    } : undefined;
  }, [snapshot.current_plan, subagents.history?.currentPlan, subagents.selected]);

  return { chat: childChat ?? mainChat, visiblePlan };
}

type MappedChildItem = { source: string; message: ChatMessage };

/**
 * Each Subagent history update replaces every item object, unlike the Task snapshot, which
 * patches items in place. Reusing the mapped message of an unchanged item keeps its identity,
 * so a settled activity group is reused rather than rebuilt and rerendered on every update.
 */
function stableChildItems(previous: Map<string, MappedChildItem>, items: ChatItem[], createdAt: string) {
  const cache = new Map<string, MappedChildItem>();
  const mapped = items.map((item) => {
    const source = JSON.stringify(item);
    const reused = previous.get(item.messageId);
    const entry = reused?.source === source ? reused : { source, message: mapProtocolChatItem(item, createdAt) };
    cache.set(item.messageId, entry);
    return entry.message;
  });
  return { cache, items: mapped };
}
