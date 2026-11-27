import type { ReplicaSnapshot } from "./backendConnection.js";

export type ResyncReason =
  | "missingChatItem"
  | "taskRevisionGap"
  | "toolDetailRevisionGap"
  | "subagentHistoryRevisionGap"
  | "missingNavigationRow"
  | "missingNavigationGroup";

/**
 * Safe, metadata-only facts about a rejected event for telemetry: revisions and counts, plus
 * opaque row/group ids (never titles, labels or content). Fields are present when at hand.
 */
export type RejectionDetail = {
  /** Revision the rejected event carried. */
  eventRevision?: number;
  /** Revision of the copy the client holds. */
  heldRevision?: number;
  /** Navigation rows a frame ordered but neither carried nor had a held body for. */
  missingRowCount?: number;
  /** A capped sample of those rows' opaque ids. */
  missingRowIds?: string[];
  /** Navigation groups the frame's group order named but the client could not resolve. */
  missingGroupCount?: number;
};

export type SnapshotUpdate =
  | { kind: "updated"; snapshot: ReplicaSnapshot; changed: boolean }
  | { kind: "resyncRequired"; reason: ResyncReason; detail: RejectionDetail };

export function changed(snapshot: ReplicaSnapshot): SnapshotUpdate {
  return { kind: "updated", snapshot, changed: true };
}

export function unchanged(snapshot: ReplicaSnapshot): SnapshotUpdate {
  return { kind: "updated", snapshot, changed: false };
}
