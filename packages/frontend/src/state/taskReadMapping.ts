import type { TaskChatPageResult } from "@openaide/app-server-client";
import type { MessagePage } from "@openaide/app-shell-contracts";
import { mapProtocolChatItem } from "./appServerProtocolChatMapping";

export function mapProtocolChatPage(page: TaskChatPageResult, createdAt: string): MessagePage {
  return {
    task_id: page.taskId,
    items: page.items.map((item) => mapProtocolChatItem(item, createdAt)),
    has_before: page.hasBefore,
    has_messages: page.totalCount > 0,
    total_count: page.totalCount,
    version: page.revision,
    start_cursor: page.startCursor ?? undefined,
    end_cursor: page.endCursor ?? undefined,
  };
}
