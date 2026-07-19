import type {
  AgentListSessionsResult,
  Attachment,
  ConfigOptionsCatalog,
  IsolationKind,
} from "@openaide/app-shell-contracts";
import {
  invalidateAppServerAttachments,
  localAttachment,
  selectionWithAgent,
  selectionWithConfigOptions,
  selectionWithIsolation,
  selectionWithProject,
  selectionWithWorkspace,
  type ProjectOption,
  type WorkspaceRoot,
} from "./composerOptions";
import type { AppAction } from "./appReducer";
import type { AppState } from "./store";
import { newTaskPreparationKey } from "./newTaskPreparationContext";

type NewTaskAction =
  | { type: "prompt"; prompt: string }
  | { type: "submit:start"; prompt?: string; context?: AppState["newTask"]["context"] }
  | { type: "submit:cancel" }
  | { type: "submit:error"; message: string }
  | { type: "submit:attachments:invalidate"; taskId: string; message: string }
  | { type: "newTask:reset" }
  | { type: "newTask:prepared"; taskId: string }
  | { type: "newTask:leaseExpired"; taskId: string; message: string }
  | { type: "newTask:agent"; agentId: string; agentLabel?: string; newTaskId?: string }
  | { type: "newTask:project"; project: ProjectOption; newTaskId?: string }
  | { type: "newTask:projectId"; projectId: string; newTaskId?: string }
  | { type: "newTask:isolation"; isolation: IsolationKind }
  | { type: "newTask:configOptions:start" }
  | { type: "newTask:configOptions:result"; catalog: ConfigOptionsCatalog }
  | { type: "newTask:configOptions:error"; message: string }
  | { type: "newTask:nativeSessions:start"; append: boolean }
  | { type: "newTask:nativeSessions:result"; result: AgentListSessionsResult; append: boolean }
  | { type: "newTask:nativeSessions:listError"; message: string }
  | { type: "newTask:nativeSessions:error"; sessionId: string; message: string }
  | { type: "newTask:nativeSessions:adopt"; sessionId: string }
  | { type: "newTask:nativeSessions:remove"; sessionId: string }
  | { type: "newTask:workspace"; workspace: WorkspaceRoot; newTaskId?: string }
  | { type: "newTask:worktree"; worktreeId?: string; label: string; path: string; newTaskId?: string }
  | { type: "newTask:attachment:add"; attachment: Attachment }
  | { type: "newTask:attachment:remove"; attachmentId: string };

export function reduceNewTaskState(state: AppState, action: AppAction): AppState | undefined {
  if (!isNewTaskAction(action)) return undefined;
  switch (action.type) {
    case "prompt":
      return { ...state, newTask: { ...state.newTask, prompt: action.prompt } };
    case "submit:start": {
      const submittedPrompt = action.prompt ?? state.newTask.prompt;
      const submittedContext = action.context ?? state.newTask.context;
      return {
        ...state,
        newTask: {
          ...state.newTask,
          prompt: submittedPrompt,
          context: submittedContext,
          pending: {
            prompt: submittedPrompt,
            context: submittedContext,
            configOptions: state.newTask.configOptions,
          },
          submitting: true,
          error: undefined,
        },
      };
    }
    case "submit:error":
      return {
        ...state,
        newTask: {
          ...state.newTask,
          prompt: state.newTask.pending?.prompt ?? state.newTask.prompt,
          context: state.newTask.pending?.context ?? state.newTask.context,
          pending: undefined,
          submitting: false,
          error: action.message,
          nativeSessions: { ...state.newTask.nativeSessions, adoptingSessionId: undefined },
        },
      };
    case "submit:cancel":
      return {
        ...state,
        newTask: {
          ...state.newTask,
          prompt: state.newTask.pending?.prompt ?? state.newTask.prompt,
          context: state.newTask.pending?.context ?? state.newTask.context,
          pending: undefined,
          submitting: false,
          error: undefined,
          nativeSessions: { ...state.newTask.nativeSessions, adoptingSessionId: undefined },
        },
      };
    case "submit:attachments:invalidate": {
      const pending = state.newTask.pending ?? {
        prompt: state.newTask.prompt,
        context: state.newTask.context,
      };
      const context = invalidateAppServerAttachments(pending.context, action.message);
      return {
        ...state,
        newTask: {
          ...state.newTask,
          prompt: pending.prompt,
          context,
          pending: undefined,
          submitting: false,
          error: action.message,
          nativeSessions: { ...state.newTask.nativeSessions, adoptingSessionId: undefined },
        },
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: {
            prompt: pending.prompt,
            context,
            error: action.message,
          },
        },
      };
    }
    case "newTask:reset":
      return {
        ...state,
        newTask: {
          ...state.newTask,
          prompt: "",
          context: [],
          pending: undefined,
          submitting: false,
          error: undefined,
          nativeSessions: { ...state.newTask.nativeSessions, adoptingSessionId: undefined },
        },
      };
    case "newTask:prepared": {
      return state;
    }
    case "newTask:leaseExpired": {
      const preparedInput = state.taskInputs[action.taskId];
      const draft = preparedInput?.pending ?? preparedInput;
      const { [action.taskId]: _expiredInput, ...taskInputs } = state.taskInputs;
      return {
        ...state,
        newTask: {
          ...state.newTask,
          prompt: draft?.prompt ?? state.newTask.prompt,
          context: invalidateAppServerAttachments(
            draft?.context ?? state.newTask.context,
            action.message,
          ),
          pending: undefined,
          submitting: false,
        },
        taskInputs,
      };
    }
    case "newTask:agent":
      return replacePreparedDraftOnContextChange(state, {
          ...state.newTask,
          selection: { ...selectionWithAgent(state.newTask.selection, action.agentId, action.agentLabel), configOptions: {} },
          configOptions: undefined,
          configOptionsLoading: false,
          configOptionsError: undefined,
          nativeSessions: emptyNativeSessions(),
      }, action.newTaskId);
    case "newTask:project":
      return replacePreparedDraftOnContextChange(state, {
          ...state.newTask,
          selection: { ...selectionWithProject(state.newTask.selection, action.project), configOptions: {} },
          configOptions: undefined,
          configOptionsLoading: false,
          configOptionsError: undefined,
          nativeSessions: emptyNativeSessions(),
      }, action.newTaskId);
    case "newTask:projectId": {
      const project = state.projects.find((candidate) => candidate.projectId === action.projectId);
      const sameProject = state.newTask.selection.projectId === action.projectId;
      return replacePreparedDraftOnContextChange(state, {
          ...state.newTask,
          selection: {
            ...state.newTask.selection,
            projectId: action.projectId,
            workspaceLabel: project?.label ?? state.newTask.selection.workspaceLabel,
            configOptions: sameProject ? state.newTask.selection.configOptions : {},
          },
          configOptions: sameProject ? state.newTask.configOptions : undefined,
          configOptionsLoading: sameProject ? state.newTask.configOptionsLoading : false,
          configOptionsError: sameProject ? state.newTask.configOptionsError : undefined,
          nativeSessions: sameProject ? state.newTask.nativeSessions : emptyNativeSessions(),
      }, action.newTaskId);
    }
    case "newTask:isolation":
      return {
        ...state,
        newTask: { ...state.newTask, selection: selectionWithIsolation(state.newTask.selection, action.isolation) },
      };
    case "newTask:configOptions:start":
      return {
        ...state,
        newTask: { ...state.newTask, configOptionsLoading: true, configOptionsError: undefined },
      };
    case "newTask:configOptions:result":
      return {
        ...state,
        newTask: {
          ...state.newTask,
          configOptions: action.catalog,
          configOptionsLoading: false,
          configOptionsError: undefined,
          selection: selectionWithConfigOptions(state.newTask.selection, action.catalog),
        },
      };
    case "newTask:configOptions:error":
      return {
        ...state,
        newTask: {
          ...state.newTask,
          configOptionsLoading: false,
          configOptionsError: action.message,
        },
      };
    case "newTask:nativeSessions:start":
      return {
        ...state,
        newTask: {
          ...state.newTask,
          nativeSessions: {
            ...state.newTask.nativeSessions,
            loading: true,
            error: undefined,
            recoveryKind: undefined,
          },
        },
      };
    case "newTask:nativeSessions:result": {
      const prior = action.append ? state.newTask.nativeSessions.items : [];
      const merged = new Map(prior.map((session) => [session.session_id, session]));
      for (const session of action.result.sessions) {
        merged.set(session.session_id, session);
      }
      return {
        ...state,
        newTask: {
          ...state.newTask,
          nativeSessions: {
            ...state.newTask.nativeSessions,
            items: [...merged.values()],
            loading: false,
            loaded: true,
            nextCursor: action.result.next_cursor,
            error: undefined,
          },
        },
      };
    }
    case "newTask:nativeSessions:listError":
      return {
        ...state,
        newTask: {
          ...state.newTask,
          nativeSessions: {
            ...state.newTask.nativeSessions,
            loading: false,
            loaded: true,
            error: action.message,
            recoveryKind: action.recoveryKind,
          },
        },
      };
    case "newTask:nativeSessions:error":
      if (state.newTask.nativeSessions.adoptingSessionId !== action.sessionId) return state;
      return {
        ...state,
        newTask: {
          ...state.newTask,
          submitting: false,
          nativeSessions: {
            ...state.newTask.nativeSessions,
            adoptingSessionId: undefined,
            loading: false,
            loaded: true,
            error: action.message,
          },
        },
      };
    case "newTask:nativeSessions:adopt":
      return {
        ...state,
        newTask: {
          ...state.newTask,
          submitting: true,
          error: undefined,
          nativeSessions: {
            ...state.newTask.nativeSessions,
            adoptingSessionId: action.sessionId,
            error: undefined,
          },
        },
      };
    case "newTask:nativeSessions:remove": {
      const settlesCurrentAdoption = state.newTask.nativeSessions.adoptingSessionId === action.sessionId;
      return {
        ...state,
        newTask: {
          ...state.newTask,
          submitting: settlesCurrentAdoption ? false : state.newTask.submitting,
          nativeSessions: {
            ...state.newTask.nativeSessions,
            adoptingSessionId: settlesCurrentAdoption
              ? undefined
              : state.newTask.nativeSessions.adoptingSessionId,
            items: state.newTask.nativeSessions.items.filter((session) => session.session_id !== action.sessionId),
          },
        },
      };
    }
    case "newTask:workspace":
      return replacePreparedDraftOnContextChange(state, {
          ...state.newTask,
          selection: { ...selectionWithWorkspace(state.newTask.selection, action.workspace), configOptions: {} },
          configOptions: undefined,
          configOptionsLoading: false,
          configOptionsError: undefined,
          nativeSessions: emptyNativeSessions(),
      }, action.newTaskId);
    case "newTask:worktree":
      return replacePreparedDraftOnContextChange(state, {
        ...state.newTask,
        selection: {
          ...state.newTask.selection,
          worktreeId: action.worktreeId,
          workspaceLabel: action.label,
          workspaceRoot: action.path,
          isolation: action.worktreeId ? "git_worktree" : "local",
          configOptions: {},
        },
        configOptions: undefined,
        configOptionsLoading: false,
        configOptionsError: undefined,
      }, action.newTaskId);
    case "newTask:attachment:add":
      return {
        ...state,
        newTask: {
          ...state.newTask,
          context: [...state.newTask.context, localAttachment(action.attachment)],
        },
      };
    case "newTask:attachment:remove":
      return {
        ...state,
        newTask: {
          ...state.newTask,
          context: state.newTask.context.filter((attachment) => attachment.local_id !== action.attachmentId),
        },
      };
  }
}

/** Moves the whole Frontend-owned draft across Prepared-Task lease changes. */
function replacePreparedDraftOnContextChange(
  state: AppState,
  nextNewTask: AppState["newTask"],
  newTaskId?: string,
) {
  if (newTaskPreparationKey({ newTask: state.newTask }) === newTaskPreparationKey({ newTask: nextNewTask })) {
    return { ...state, newTask: nextNewTask };
  }
  const preparedTaskId = newTaskId;
  if (!preparedTaskId) return { ...state, newTask: nextNewTask };
  const preparedInput = state.taskInputs[preparedTaskId];
  const { [preparedTaskId]: _discardedInput, ...taskInputs } = state.taskInputs;
  return {
    ...state,
    newTask: {
      ...nextNewTask,
      prompt: preparedInput?.prompt ?? nextNewTask.prompt,
      context: preparedInput?.context ?? nextNewTask.context,
    },
    taskInputs,
  };
}

function isNewTaskAction(action: AppAction): action is NewTaskAction {
  return action.type === "prompt"
    || action.type === "submit:start"
    || action.type === "submit:cancel"
    || action.type === "submit:error"
    || action.type === "submit:attachments:invalidate"
    || action.type.startsWith("newTask:");
}

function emptyNativeSessions() {
  return {
    items: [],
    loading: false,
    loaded: false,
  };
}
