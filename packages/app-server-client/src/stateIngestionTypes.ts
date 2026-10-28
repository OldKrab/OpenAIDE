import type { ReplicaSnapshot } from "./backendConnection.js";

export type SnapshotUpdate =
  | { kind: "updated"; snapshot: ReplicaSnapshot; changed: boolean }
  | { kind: "resyncRequired"; reason: "missingChatItem" | "taskRevisionGap" | "toolDetailRevisionGap" };

export function changed(snapshot: ReplicaSnapshot): SnapshotUpdate {
  return { kind: "updated", snapshot, changed: true };
}

export function unchanged(snapshot: ReplicaSnapshot): SnapshotUpdate {
  return { kind: "updated", snapshot, changed: false };
}
