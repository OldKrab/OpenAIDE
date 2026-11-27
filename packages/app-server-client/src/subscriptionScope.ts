import type { SubscriptionScope } from "./generated/protocol.js";

export function subscriptionScopesEqual(left: SubscriptionScope, right: SubscriptionScope): boolean {
  switch (left.kind) {
    case "taskNavigation":
      return right.kind === "taskNavigation" && left.section === right.section;
    case "task":
      return right.kind === "task" && left.taskId === right.taskId;
    case "subagentCatalog":
      return right.kind === "subagentCatalog" && left.taskId === right.taskId;
    case "subagentHistory":
      return right.kind === "subagentHistory"
        && left.taskId === right.taskId
        && left.subagentId === right.subagentId;
    case "toolDetail":
      return right.kind === "toolDetail"
        && left.taskId === right.taskId
        && left.artifactId === right.artifactId;
  }
}
