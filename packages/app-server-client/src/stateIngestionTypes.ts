import type { SubscriptionSnapshot } from "./generated/protocol.js";

export type SnapshotUpdate =
  | { kind: "updated"; snapshot: SubscriptionSnapshot; changed: boolean }
  | { kind: "resyncRequired"; reason: "missingChatItem" };

export function changed(snapshot: SubscriptionSnapshot): SnapshotUpdate {
  return { kind: "updated", snapshot, changed: true };
}

export function unchanged(snapshot: SubscriptionSnapshot): SnapshotUpdate {
  return { kind: "updated", snapshot, changed: false };
}
