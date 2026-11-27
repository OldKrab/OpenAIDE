import {
  type AgentSummary,
  type AppServerEvent,
  type AppServerSession,
  type ClientSnapshot,
  type ProjectSummary,
  type SubscriptionIngestionContext,
  type SubscriptionScope,
  type ReplicaSnapshot,
  type NavigationReplica,
  type NavigationViewParams,
} from "@openaide/app-server-client";
import type { Dispatch } from "react";
import type { AppAction } from "../state/appReducer";
import {
  createProtocolTaskSnapshotMapper,
  mapProtocolTaskNavigation,
  mapProtocolTaskSummary,
} from "../state/appServerProtocolMapping";
import { mapProtocolToolDetail } from "../state/appServerProtocolChatMapping";
import type { AgentOption } from "../state/composerOptions";

type StateSubscriptionConnection = Pick<AppServerSession, "subscribeState">;

export type StateSubscriptionMappingContext = SubscriptionIngestionContext & {
  agents?: AgentSummary[];
  projects?: ProjectSummary[];
  taskNavigations?: Partial<Record<"tasks" | "archive", NavigationReplica>>;
};

export function mappingContextFromClientSnapshot(snapshot: ClientSnapshot): StateSubscriptionMappingContext {
  return {
    stateRootId: snapshot.stateRoot.stateRootId,
    clientInstanceId: snapshot.client.clientInstanceId,
    agents: snapshot.global.agents.agents,
    projects: snapshot.global.projects.projects,
  };
}

/** Maps the session-owned scope replica into Frontend presentation actions. */
export function startAppServerStateSubscription({
  backendConnection,
  context,
  currentAgentId,
  dispatch,
  onBaselineError,
  onBaselineLost,
  onBaselineReady,
  onTaskRemoved,
  setAgents,
  scope,
  view,
}: {
  backendConnection: StateSubscriptionConnection;
  context: StateSubscriptionMappingContext;
  currentAgentId?: () => string;
  dispatch: Dispatch<AppAction>;
  onBaselineLost?: () => void;
  onBaselineError?: (error: unknown) => void;
  onBaselineReady?: () => void;
  onTaskRemoved?: (taskId: string) => void;
  setAgents?: (agents: AgentOption[]) => void;
  scope: SubscriptionScope;
  /** The parameters of a Navigation view; required when `scope` is a `taskNavigation` scope. */
  view?: NavigationViewParams;
}) {
  const mapTaskSnapshot = createProtocolTaskSnapshotMapper();
  const observer: Parameters<StateSubscriptionConnection["subscribeState"]>[1] = {
    onBaselineError,
    onBaselineLost,
    onBaselineReady,
    onSnapshot(snapshot, event, snapshotChanged = true) {
      const removedTaskId = event?.payload.kind === "taskChanged"
        && event.payload.changes.removed
        ? event.payload.taskId
        : undefined;
      if (removedTaskId) {
        dispatch({ type: "task:list:remove", taskId: removedTaskId });
        onTaskRemoved?.(removedTaskId);
        return;
      }
      const liveText = event ? liveTextPresentationAction(event, snapshot) : undefined;
      if (!snapshotChanged) {
        if (liveText) dispatch(liveText);
        return;
      }
      for (const action of actionsFromSubscriptionSnapshot(snapshot, context, {
        currentAgentId,
        dispatch,
        setAgents,
      }, mapTaskSnapshot)) {
        if (
          action.type === "snapshot"
          && liveText
          && action.snapshot.task.task_id === liveText.taskId
        ) {
          dispatch({
            ...action,
            liveText: {
              messageId: liveText.messageId,
              channel: liveText.channel,
              eventCursor: liveText.eventCursor,
            },
          });
        } else {
          dispatch(action);
        }
      }
    },
  };
  return view
    ? backendConnection.subscribeState(scope, observer, view)
    : backendConnection.subscribeState(scope, observer);
}

function liveTextPresentationAction(
  event: AppServerEvent,
  snapshot: ReplicaSnapshot,
): Extract<AppAction, { type: "taskChat:liveText" }> | undefined {
  if (snapshot.kind !== "task") return undefined;
  const payload = event.payload;
  if (payload.kind !== "taskChanged") return undefined;
  const liveChange = [...(payload.changes.chat ?? [])].reverse().find((change) => (
    change.kind === "append" || change.kind === "appendText"
  ));
  if (!liveChange) return undefined;
  const messageId = liveChange.kind === "append" ? liveChange.item.messageId : liveChange.messageId;
  const item = liveChange.kind === "append"
    ? liveChange.item
    : snapshot.task.chat.items.find((candidate) => candidate.messageId === messageId);
  const channel = item && textChannel(item);
  if (!channel) return undefined;
  return {
    type: "taskChat:liveText",
    taskId: payload.taskId,
    messageId,
    channel,
    eventCursor: event.cursor,
  };
}

function textChannel(item: import("@openaide/app-server-client").ChatItem) {
  if (!item.parts.some((part) => part.kind === "text")) return undefined;
  if (item.role === "agent") return "agent" as const;
  if (item.role === "system") return "thought" as const;
  return undefined;
}

function actionsFromSubscriptionSnapshot(
  snapshot: ReplicaSnapshot,
  context: StateSubscriptionMappingContext,
  agents: {
    currentAgentId?: () => string;
    dispatch: Dispatch<AppAction>;
    setAgents?: (agents: AgentOption[]) => void;
  },
  mapTaskSnapshot = createProtocolTaskSnapshotMapper(),
): AppAction[] {
  switch (snapshot.kind) {
    case "taskNavigation":
      context.taskNavigations = {
        ...context.taskNavigations,
        [snapshot.navigation.section]: snapshot.navigation,
      };
      return remappedTaskNavigationActions(context);
    case "task": {
      const mapped = mapTaskSnapshot(snapshot.task, context);
      return [{ type: "snapshot", snapshot: mapped.snapshot, intent: "refresh" }];
    }
    case "toolDetail":
      return [{
        type: "toolDetail:result",
        taskId: snapshot.taskId,
        artifactId: snapshot.artifactId,
        details: mapProtocolToolDetail(snapshot.details),
      }];
    case "subagentCatalog":
    case "subagentHistory":
      // Focused Task inspection owns these independently ordered replicas.
      return [];
  }
}

export function remappedTaskNavigationActions(context: StateSubscriptionMappingContext): AppAction[] {
  return (["tasks", "archive"] as const).flatMap((section): AppAction[] => {
    const navigation = context.taskNavigations?.[section];
    if (!navigation) return [];
    const mapped = mapProtocolTaskNavigation(navigation, context);
    const actions: AppAction[] = [{
      type: "tasks",
      archived: section === "archive",
      tasks: mapped.tasks,
    }];
    actions.push({
      type: "taskNavigation",
      archived: section === "archive",
      tasks: mapped.tasks,
      sessions: mapped.sessions,
      groups: mapped.groups,
      refreshing: mapped.refreshing,
      refreshError: mapped.refreshError,
      recoveryKind: mapped.recoveryKind,
      recoveryAgentId: mapped.recoveryAgentId,
      recoveryAgentLabel: mapped.recoveryAgentLabel,
    });
    return actions;
  });
}
