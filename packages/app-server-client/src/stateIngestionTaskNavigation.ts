import type {
  NavigationDiscovery,
  NavigationFrame,
  NavigationRow,
  NavigationSubscribeResult,
  ProjectId,
  TaskNavigationSection,
} from "./generated/protocol.js";
import type { RejectionDetail } from "./stateIngestionTypes.js";

/** One Project's rows in a Navigation view, exactly as App Server last sent them. */
export type NavigationGroupReplica = {
  projectId: ProjectId;
  projectLabel: string;
  /** The server's complete window order, as row ids. The client never sorts or limits. */
  order: string[];
  /** The bodies of the rows in `order`. */
  rowsById: Record<string, NavigationRow>;
  /** Rows the Project has in this section after the membership filters. */
  total: number;
  /** Rows outside the window. */
  hidden: number;
  undiscovered: boolean;
  discovery: NavigationDiscovery;
};

/** A Navigation view's installed state: groups in server order. */
export type NavigationReplica = {
  section: TaskNavigationSection;
  /** The revision of the last frame applied; a frame at or below it is stale. */
  revision: number;
  groups: NavigationGroupReplica[];
};

/** A group's rows in the server's order. */
export function navigationGroupRows(group: NavigationGroupReplica): NavigationRow[] {
  return group.order.map((rowId) => group.rowsById[rowId]!);
}

export type NavigationFrameApplication =
  | { kind: "applied"; navigation: NavigationReplica }
  /** The frame was sent before the baseline the client holds. */
  | { kind: "stale" }
  | {
      kind: "resyncRequired";
      reason: "missingNavigationRow" | "missingNavigationGroup";
      detail: RejectionDetail;
    };

/** Bounds the opaque row ids a rejection reports. */
const MAX_REPORTED_MISSING_ROWS = 5;

/** Installs a `navigation/subscribe` reply, whose frame is the complete baseline. */
export function installNavigationReply(result: NavigationSubscribeResult): NavigationReplica | undefined {
  const applied = applyNavigationFrame(undefined, result.frame);
  return applied.kind === "applied" ? applied.navigation : undefined;
}

/**
 * Applies one frame: each changed group's rows are `order.map(id => frame.rows[id] ?? previous[id])`.
 * There are no insert, remove or move operations, so a frame cannot leave the view in a state the
 * server did not compute. A row the frame omits and the client never held means a lost frame.
 * A frame whose revision is not above the held one was sent before the client's baseline (it
 * overtook the subscribe reply) and is dropped.
 */
export function applyNavigationFrame(
  previous: NavigationReplica | undefined,
  frame: NavigationFrame,
): NavigationFrameApplication {
  if (previous && frame.revision <= previous.revision) return { kind: "stale" };
  const known = new Map<ProjectId, NavigationGroupReplica>(
    (previous?.groups ?? []).map((group) => [group.projectId, group]),
  );
  for (const projectId of frame.removedGroups) known.delete(projectId);
  const missingRowIds: string[] = [];
  for (const changed of frame.groups) {
    const before = known.get(changed.projectId);
    const rowsById: Record<string, NavigationRow> = {};
    for (const rowId of changed.order) {
      const row = changed.rows[rowId] ?? before?.rowsById[rowId];
      if (!row) {
        missingRowIds.push(rowId);
        continue;
      }
      rowsById[rowId] = row;
    }
    known.set(changed.projectId, {
      projectId: changed.projectId,
      projectLabel: changed.projectLabel,
      order: changed.order,
      rowsById,
      total: changed.total,
      hidden: changed.hidden,
      undiscovered: changed.undiscovered,
      discovery: changed.discovery,
    });
  }
  if (missingRowIds.length > 0) {
    return {
      kind: "resyncRequired",
      reason: "missingNavigationRow",
      detail: {
        eventRevision: frame.revision,
        heldRevision: previous?.revision,
        missingRowCount: missingRowIds.length,
        missingRowIds: missingRowIds.slice(0, MAX_REPORTED_MISSING_ROWS),
      },
    };
  }
  const groupOrder = frame.groupOrder ?? previous?.groups.map((group) => group.projectId) ?? [];
  const groups: NavigationGroupReplica[] = [];
  let missingGroupCount = 0;
  for (const projectId of groupOrder) {
    const group = known.get(projectId);
    if (!group) missingGroupCount += 1;
    else groups.push(group);
  }
  if (missingGroupCount > 0) {
    return {
      kind: "resyncRequired",
      reason: "missingNavigationGroup",
      detail: { eventRevision: frame.revision, heldRevision: previous?.revision, missingGroupCount },
    };
  }
  return { kind: "applied", navigation: { section: frame.section, revision: frame.revision, groups } };
}
