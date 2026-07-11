import {
  applySubscriptionEvent,
  createSubscriptionIngestionState,
  STATE_SUBSCRIBE,
  STATE_UNSUBSCRIBE,
  type AgentSummary,
  type AppServerEvent,
  type BackendConnection,
  type ClientSnapshot,
  type ProjectSummary,
  type SubscriptionIngestionContext,
  type SubscriptionIngestionState,
  type SubscriptionScope,
  type SubscriptionSnapshot,
} from "@openaide/app-server-client";
import type { Dispatch } from "react";
import type { AppAction } from "../state/appReducer";
import { applyProtocolAgents } from "../state/appServerAgents";
import {
  mapProtocolTaskNavigation,
  mapProtocolTaskSnapshot,
} from "../state/appServerProtocolMapping";
import type { AgentOption } from "../state/composerOptions";

type StateSubscriptionConnection = Pick<BackendConnection, "events" | "request">;

export type StateSubscriptionMappingContext = SubscriptionIngestionContext & {
  agents?: AgentSummary[];
  projects?: ProjectSummary[];
};

export function mappingContextFromClientSnapshot(snapshot: ClientSnapshot): StateSubscriptionMappingContext {
  return {
    stateRootId: snapshot.stateRoot.stateRootId,
    clientInstanceId: snapshot.client.clientInstanceId,
    agents: snapshot.agents?.agents,
    projects: snapshot.projects?.projects,
  };
}

export function startAppServerStateSubscription({
  backendConnection,
  context,
  currentAgentId,
  dispatch,
  setAgents,
  scope,
}: {
  backendConnection: StateSubscriptionConnection;
  context: StateSubscriptionMappingContext;
  currentAgentId?: () => string;
  dispatch: Dispatch<AppAction>;
  setAgents?: (agents: AgentOption[]) => void;
  scope: SubscriptionScope;
}) {
  let disposed = false;
  let state: SubscriptionIngestionState | undefined;
  let pendingEvents: AppServerEvent[] = [];
  let unsubscribe = backendConnection.events(handleEvent);
  let subscribeInFlight = false;

  void subscribe();

  return () => {
    disposed = true;
    unsubscribe();
    void backendConnection.request(STATE_UNSUBSCRIBE, { scope }).catch(() => {
      // Connection cleanup is best-effort; reconnect/expiry paths still clear stale subscribers.
    });
  };

  async function subscribe() {
    if (subscribeInFlight) return;
    subscribeInFlight = true;
    try {
      const result = await backendConnection.request(STATE_SUBSCRIBE, { scope });
      if (disposed) return;
      state = createSubscriptionIngestionState(result, context);
      applySnapshot(result.snapshot);
      replayPendingEvents();
    } catch {
      // The next explicit request will still reconcile state through response snapshots.
    } finally {
      subscribeInFlight = false;
    }
  }

  function handleEvent(event: AppServerEvent) {
    if (disposed) return;
    if (!state) {
      pendingEvents.push(event);
      return;
    }
    applyEvent(event);
  }

  function applyEvent(event: AppServerEvent) {
    if (!state || disposed) return;
    const result = applySubscriptionEvent(state, event);
    if (result.kind === "ignored") return;
    if (result.kind === "resyncRequired") {
      void subscribe();
      return;
    }
    state = result.state;
    if (result.snapshotChanged) applySnapshot(result.state.snapshot);
  }

  function replayPendingEvents() {
    if (!state || pendingEvents.length === 0) return;
    const snapshotCursorIndex = pendingEvents.findIndex((event) => event.cursor === state?.cursor);
    const events = snapshotCursorIndex === -1
      ? pendingEvents
      : pendingEvents.slice(snapshotCursorIndex + 1);
    pendingEvents = [];
    for (const event of events) applyEvent(event);
  }

  function applySnapshot(snapshot: SubscriptionSnapshot) {
    for (const action of actionsFromSubscriptionSnapshot(snapshot, context, {
      currentAgentId,
      dispatch,
      setAgents,
    })) {
      dispatch(action);
    }
  }
}

function actionsFromSubscriptionSnapshot(
  snapshot: SubscriptionSnapshot,
  context: StateSubscriptionMappingContext,
  agents: {
    currentAgentId?: () => string;
    dispatch: Dispatch<AppAction>;
    setAgents?: (agents: AgentOption[]) => void;
  },
): AppAction[] {
  switch (snapshot.kind) {
    case "projects":
      context.projects = snapshot.projects.projects;
      return [{
        type: "projects",
        activeProjectId: snapshot.projects.activeProjectId ?? undefined,
        projects: snapshot.projects.projects.map((project) => ({
          projectId: project.projectId,
          label: project.label,
        })),
      }];
    case "agents":
      context.agents = snapshot.agents.agents;
      if (agents.setAgents) {
        applyProtocolAgents(snapshot.agents, agents.currentAgentId?.() ?? "", agents.setAgents, agents.dispatch);
      }
      return [];
    case "taskNavigation": {
      const mapped = mapProtocolTaskNavigation(snapshot.navigation, context);
      return [{ type: "tasks", tasks: mapped.tasks }];
    }
    case "task": {
      const mapped = mapProtocolTaskSnapshot(snapshot.task, context);
      return [{ type: "snapshot", snapshot: mapped.snapshot, intent: "refresh" }];
    }
    case "settings":
      return [];
  }
}
