import type { ActivityToolDetails, Attachment, ChatMessage, MessagePage } from "@openaide/app-shell-contracts";
import { mergePageState } from "./chatPaging";
import { invalidateAppServerAttachments, localAttachment } from "./composerOptions";
import type { ComposerAttachment } from "./composerOptions";
import type { AppAction } from "./appReducer";
import { toolDetailCacheKey, type AppState } from "./store";

type TaskInteractionAction =
  | { type: "taskInput:prompt"; taskId: string; prompt: string }
  | { type: "taskInput:attachment:add"; taskId: string; attachment: Attachment }
  | { type: "taskInput:attachment:addAppServer"; taskId: string; attachment: ComposerAttachment }
  | { type: "taskInput:attachment:remove"; taskId: string; attachmentId: string }
  | { type: "taskInput:clear"; taskId: string }
  | { type: "taskInput:submit"; taskId: string; input?: { prompt: string; context: ComposerAttachment[] } }
  | { type: "taskInput:error"; taskId: string; message?: string }
  | { type: "taskInput:attachments:invalidate"; taskId: string; message: string }
  | { type: "taskOpen:error"; taskId: string; message: string }
  | { type: "chatPage:start"; taskId: string }
  | { type: "chatPage:result"; taskId: string; page: MessagePage }
  | { type: "chatPage:error"; taskId: string; message: string }
  | { type: "toolDetail:start"; taskId: string; artifactId: string }
  | { type: "toolDetail:result"; taskId: string; artifactId: string; details: ActivityToolDetails }
  | { type: "toolDetail:error"; taskId: string; artifactId: string; message: string }
  | { type: "permission:responding"; requestId: string }
  | { type: "permission:error"; requestId: string; message: string }
  | { type: "appServerPermission:received"; requestId: string; message: ChatMessage; taskId?: string }
  | { type: "appServerPermission:resolved"; requestId: string }
  | { type: "question:responding"; requestId: string }
  | { type: "question:error"; requestId: string; message: string }
  | { type: "appServerQuestion:received"; requestId: string; message: ChatMessage; taskId?: string }
  | { type: "appServerQuestion:resolved"; requestId: string };

export function reduceTaskInteractionState(state: AppState, action: AppAction): AppState | undefined {
  if (!isTaskInteractionAction(action)) return undefined;
  switch (action.type) {
    case "taskInput:prompt":
      const input = state.taskInputs[action.taskId];
      return {
        ...state,
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: {
            ...input,
            prompt: action.prompt,
            context: input?.context ?? [],
            error: undefined,
          },
        },
      };
    case "taskInput:attachment:add": {
      const input = state.taskInputs[action.taskId] ?? { prompt: "", context: [] };
      return {
        ...state,
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: {
            ...input,
            context: [...input.context, localAttachment(action.attachment)],
            error: undefined,
          },
        },
      };
    }
    case "taskInput:attachment:addAppServer": {
      const input = state.taskInputs[action.taskId] ?? { prompt: "", context: [] };
      return {
        ...state,
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: {
            ...input,
            context: [...input.context, action.attachment],
            error: undefined,
          },
        },
      };
    }
    case "taskInput:attachment:remove": {
      const input = state.taskInputs[action.taskId] ?? { prompt: "", context: [] };
      return {
        ...state,
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: {
            ...input,
            context: input.context.filter((attachment) => attachment.local_id !== action.attachmentId),
            error: undefined,
          },
        },
      };
    }
    case "taskInput:clear": {
      const { [action.taskId]: _input, ...taskInputs } = state.taskInputs;
      return { ...state, taskInputs };
    }
    case "taskInput:submit": {
      const input = action.input ?? state.taskInputs[action.taskId] ?? { prompt: "", context: [] };
      return {
        ...state,
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: {
            prompt: "",
            context: [],
            error: undefined,
            pending: { prompt: input.prompt, context: input.context },
          },
        },
      };
    }
    case "taskInput:error": {
      const input = state.taskInputs[action.taskId];
      if (!input?.pending) {
        if (!action.message) return state;
        return {
          ...state,
          taskInputs: {
            ...state.taskInputs,
            [action.taskId]: {
              prompt: input?.prompt ?? "",
              context: input?.context ?? [],
              error: action.message,
            },
          },
        };
      }
      return {
        ...state,
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: {
            prompt: input.pending.prompt,
            context: input.pending.context,
            error: action.message,
          },
        },
      };
    }
    case "taskInput:attachments:invalidate": {
      const input = state.taskInputs[action.taskId];
      const draft = input?.pending ?? input ?? { prompt: "", context: [] };
      return {
        ...state,
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: {
            prompt: draft.prompt,
            context: invalidateAppServerAttachments(draft.context, action.message),
            error: action.message,
          },
        },
      };
    }
    case "taskOpen:error":
      if (state.snapshot || (state.activeTaskId !== undefined && state.activeTaskId !== action.taskId)) return state;
      return { ...state, taskOpenError: { taskId: action.taskId, message: action.message } };
    case "chatPage:start": {
      const current = state.chatPages[action.taskId] ?? { olderItems: [], hasBefore: true };
      return {
        ...state,
        chatPages: {
          ...state.chatPages,
          [action.taskId]: { ...current, pending: true, error: undefined },
        },
      };
    }
    case "chatPage:result": {
      if (state.snapshot?.task.task_id !== action.taskId) return state;
      return {
        ...state,
        chatPages: {
          ...state.chatPages,
          [action.taskId]: mergePageState(state.chatPages[action.taskId], action.page),
        },
      };
    }
    case "chatPage:error": {
      if (state.snapshot?.task.task_id !== action.taskId) return state;
      const current = state.chatPages[action.taskId] ?? { olderItems: [], hasBefore: true };
      return {
        ...state,
        chatPages: {
          ...state.chatPages,
          [action.taskId]: { ...current, pending: false, error: action.message },
        },
      };
    }
    case "toolDetail:start": {
      const key = toolDetailCacheKey(action.taskId, action.artifactId);
      return {
        ...state,
        toolDetails: {
          ...state.toolDetails,
          [key]: { ...state.toolDetails[key], loading: true, error: undefined },
        },
      };
    }
    case "toolDetail:result": {
      const key = toolDetailCacheKey(action.taskId, action.artifactId);
      return {
        ...state,
        toolDetails: {
          ...state.toolDetails,
          [key]: { loading: false, details: action.details },
        },
      };
    }
    case "toolDetail:error": {
      const key = toolDetailCacheKey(action.taskId, action.artifactId);
      return {
        ...state,
        toolDetails: {
          ...state.toolDetails,
          [key]: { loading: false, error: action.message },
        },
      };
    }
    case "permission:responding":
      return {
        ...state,
        permissionResponses: {
          ...state.permissionResponses,
          [action.requestId]: { responding: true },
        },
      };
    case "permission:error":
      return {
        ...state,
        permissionResponses: {
          ...state.permissionResponses,
          [action.requestId]: { responding: false, error: action.message },
        },
      };
    case "appServerPermission:received":
      return {
        ...state,
        appServerPermissionRequests: {
          ...state.appServerPermissionRequests,
          [action.requestId]: { taskId: action.taskId, message: action.message },
        },
      };
    case "appServerPermission:resolved": {
      return state;
    }
    case "question:responding":
      return {
        ...state,
        questionResponses: {
          ...state.questionResponses,
          [action.requestId]: { responding: true },
        },
      };
    case "question:error":
      return {
        ...state,
        questionResponses: {
          ...state.questionResponses,
          [action.requestId]: { responding: false, error: action.message },
        },
      };
    case "appServerQuestion:received":
      return {
        ...state,
        appServerQuestionRequests: {
          ...state.appServerQuestionRequests,
          [action.requestId]: { taskId: action.taskId, message: action.message },
        },
      };
    case "appServerQuestion:resolved":
      return state;
  }
}

function isTaskInteractionAction(action: AppAction): action is TaskInteractionAction {
  return action.type.startsWith("taskInput:")
    || action.type === "taskOpen:error"
    || action.type.startsWith("chatPage:")
    || action.type.startsWith("toolDetail:")
    || action.type.startsWith("permission:")
    || action.type === "appServerPermission:received"
    || action.type === "appServerPermission:resolved"
    || action.type.startsWith("question:")
    || action.type === "appServerQuestion:received"
    || action.type === "appServerQuestion:resolved";
}
