import type {
  TaskChatPageResult,
  TaskToolDetailResult,
} from "@openaide/app-server-client";
import type {
  ActivityToolDetails,
  MessagePage,
} from "@openaide/app-shell-contracts";
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

export function mapProtocolToolDetail(details: TaskToolDetailResult): ActivityToolDetails {
  return {
    locations: details.locations.map((location) => ({
      path: location.path,
      line: location.line ?? undefined,
    })),
    content: details.content.map((content) => {
      if (content.kind === "diff") {
        return {
          kind: "diff",
          path: content.path,
          old_text: content.oldText ?? undefined,
          new_text: content.newText,
        };
      }
      if (content.kind === "terminal") return { kind: "terminal", terminal_id: content.terminalId };
      return content;
    }),
    input: details.input
      ? {
          command: details.input.command,
          cwd: details.input.cwd ?? undefined,
          query: details.input.query ?? undefined,
          queries: details.input.queries ?? undefined,
          url: details.input.url ?? undefined,
          path: details.input.path ?? undefined,
          fields: details.input.fields,
        }
      : undefined,
    output: details.output
      ? {
          stdout: details.output.stdout ?? undefined,
          stderr: details.output.stderr ?? undefined,
          formatted_output: details.output.formattedOutput ?? undefined,
          aggregated_output: details.output.aggregatedOutput ?? undefined,
          exit_code: details.output.exitCode ?? undefined,
          success: details.output.success ?? undefined,
          fields: details.output.fields,
        }
      : undefined,
  };
}
