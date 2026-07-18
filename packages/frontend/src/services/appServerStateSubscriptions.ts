import {
  type AgentSummary,
  type AppServerEvent,
  type AppServerSession,
  type ClientSnapshot,
  type ProjectSummary,
  type SubscriptionIngestionContext,
  type SubscriptionScope,
  type SubscriptionSnapshot,
  type TaskNavigationSnapshot,
} from "@openaide/app-server-client";
import type { Dispatch } from "react";
import type { AppAction } from "../state/appReducer";
import { applyProtocolAgents } from "../state/appServerAgents";
import {
  createProtocolTaskSnapshotMapper,
  mapProtocolTaskNavigation,
} from "../state/appServerProtocolMapping";
import { mapProtocolToolDetail } from "../state/appServerProtocolChatMapping";
import type { AgentOption } from "../state/composerOptions";

type StateSubscriptionConnection = Pick<AppServerSession, "subscribeState">;

export type StateSubscriptionMappingContext = SubscriptionIngestionContext & {
  agents?: AgentSummary[];
  projects?: ProjectSummary[];
  taskNavigation?: TaskNavigationSnapshot;
};

export function mappingContextFromClientSnapshot(snapshot: ClientSnapshot): StateSubscriptionMappingContext {
  return {
    stateRootId: snapshot.stateRoot.stateRootId,
    clientInstanceId: snapshot.client.clientInstanceId,
    agents: snapshot.agents?.agents,
    projects: snapshot.projects?.projects,
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
  setAgents,
  scope,
}: {
  backendConnection: StateSubscriptionConnection;
  context: StateSubscriptionMappingContext;
  currentAgentId?: () => string;
  dispatch: Dispatch<AppAction>;
  onBaselineLost?: () => void;
  onBaselineError?: (error: unknown) => void;
  onBaselineReady?: () => void;
  setAgents?: (agents: AgentOption[]) => void;
  scope: SubscriptionScope;
}) {
  const mapTaskSnapshot = createProtocolTaskSnapshotMapper();
  return backendConnection.subscribeState(scope, {
    onBaselineError,
    onBaselineLost,
    onBaselineReady,
    onSnapshot(snapshot, event, snapshotChanged = true) {
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
  });
}

function liveTextPresentationAction(
  event: AppServerEvent,
  snapshot: SubscriptionSnapshot,
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
  snapshot: SubscriptionSnapshot,
  context: StateSubscriptionMappingContext,
  agents: {
    currentAgentId?: () => string;
    dispatch: Dispatch<AppAction>;
    setAgents?: (agents: AgentOption[]) => void;
  },
  mapTaskSnapshot = createProtocolTaskSnapshotMapper(),
): AppAction[] {
  switch (snapshot.kind) {
    case "projects":
      context.projects = snapshot.projects.projects;
      return [{
        type: "projects",
        projects: snapshot.projects.projects.map((project) => ({
          projectId: project.projectId,
          label: project.label,
        })),
      }, ...remappedTaskNavigationActions(context)];
    case "agents":
      context.agents = snapshot.agents.agents;
      if (agents.setAgents) {
        applyProtocolAgents(snapshot.agents, agents.currentAgentId?.() ?? "", agents.setAgents, agents.dispatch);
      }
      return remappedTaskNavigationActions(context);
    case "taskNavigation":
      context.taskNavigation = snapshot.navigation;
      return remappedTaskNavigationActions(context);
    case "task": {
      const mapped = mapTaskSnapshot(snapshot.task, context);
      return [{ type: "snapshot", snapshot: mapped.snapshot, intent: "refresh" }];
    }
    case "settings":
      return [];
    case "toolDetail":
      return [{
        type: "toolDetail:result",
        taskId: snapshot.taskId,
        artifactId: snapshot.artifactId,
        details: mapProtocolToolDetail(snapshot.details),
      }];
  }
}

function remappedTaskNavigationActions(context: StateSubscriptionMappingContext): AppAction[] {
  if (!context.taskNavigation) return [];
  const mapped = mapProtocolTaskNavigation(context.taskNavigation, context);
  return [{ type: "tasks", archived: false, tasks: mapped.tasks }];
}
