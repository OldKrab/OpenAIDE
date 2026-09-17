import { useMemo } from "react";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";

import { mapProtocolChatItem } from "../state/appServerProtocolChatMapping";
import { renderedChat } from "../state/chatPaging";
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
  const childChat = useMemo(() => {
    if (!subagents.selected) return undefined;
    const history = subagents.history;
    return {
      items: (history?.chat.items ?? []).map((item) => mapProtocolChatItem(item, snapshot.task.updated_at)),
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
