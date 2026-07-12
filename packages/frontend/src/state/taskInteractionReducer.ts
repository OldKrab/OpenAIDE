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
  | { type: "taskInput:submit"; taskId: string; input?: { prompt: string; context: ComposerAttachment[] }; idempotencyKey?: import("@openaide/app-server-client").TaskSendIdempotencyKey }
  | { type: "taskInput:restoreSend"; taskId: string; input: { prompt: string; context: ComposerAttachment[] }; idempotencyKey: import("@openaide/app-server-client").TaskSendIdempotencyKey }
  | { type: "taskInput:sendUncertain"; taskId: string; idempotencyKey: import("@openaide/app-server-client").TaskSendIdempotencyKey; message: string }
  | { type: "taskInput:sendError"; taskId: string; idempotencyKey: import("@openaide/app-server-client").TaskSendIdempotencyKey; message?: string }
  | { type: "taskSend:accepted"; taskId: string; idempotencyKey: import("@openaide/app-server-client").TaskSendIdempotencyKey; userMessageId: import("@openaide/app-server-client").MessageId }
  | { type: "taskInput:error"; taskId: string; message?: string }
  | { type: "taskInput:cancelError"; taskId: string; message: string }
  | { type: "taskInput:attachments:invalidate"; taskId: string; message: string }
  | { type: "taskOpen:start"; taskId: string }
  | { type: "taskOpen:error"; taskId: string; message: string }
  | { type: "chatPage:start"; taskId: string; requestGeneration: number }
  | { type: "chatPage:result"; taskId: string; requestGeneration: number; page: MessagePage }
  | { type: "chatPage:error"; taskId: string; requestGeneration: number; message: string }
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
      if (input?.pending) return state;
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
      if (input.pending) return state;
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
      if (input.pending) return state;
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
      if (input.pending) return state;
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
    case "taskInput:submit":
    case "taskInput:restoreSend": {
      const previousInput = state.taskInputs[action.taskId];
      const input = action.input ?? previousInput ?? { prompt: "", context: [] };
      return {
        ...state,
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: {
            prompt: input.prompt,
            context: input.context,
            ...acceptedInputIdentity(previousInput),
            error: undefined,
            pending: {
              prompt: input.prompt,
              context: input.context,
              idempotencyKey: action.idempotencyKey,
              state: "sending",
            },
          },
        },
      };
    }
    case "taskInput:sendUncertain": {
      const input = state.taskInputs[action.taskId];
      if (input?.pending?.idempotencyKey !== action.idempotencyKey) return state;
      return {
        ...state,
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: {
            ...input,
            error: action.message,
            pending: { ...input.pending, state: "uncertain" },
          },
        },
      };
    }
    case "taskInput:sendError": {
      const input = state.taskInputs[action.taskId];
      if (input?.pending?.idempotencyKey !== action.idempotencyKey) return state;
      return {
        ...state,
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: {
            prompt: input.pending.prompt,
            context: input.pending.context,
            ...acceptedInputIdentity(input),
            error: action.message,
          },
        },
      };
    }
    case "taskSend:accepted": {
      const input = state.taskInputs[action.taskId];
      const hasAcceptedMessage = typeof action.userMessageId === "string" && action.userMessageId.length > 0;
      const acceptedTaskInput = input?.pending?.idempotencyKey === action.idempotencyKey
        && hasAcceptedMessage;
      const acceptedNewTask = state.newTask.pending?.idempotencyKey === action.idempotencyKey
        && hasAcceptedMessage;
      if (!acceptedTaskInput && !acceptedNewTask) return state;
      return {
        ...state,
        taskInputs: acceptedTaskInput
          ? {
              ...state.taskInputs,
              [action.taskId]: {
                prompt: "",
                context: [],
                acceptedUserMessageId: action.userMessageId,
              },
            }
          : state.taskInputs,
        newTask: acceptedNewTask
          ? {
              ...state.newTask,
              prompt: "",
              context: [],
              pending: undefined,
              submitting: false,
              error: undefined,
            }
          : state.newTask,
      };
    }
    case "taskInput:error": {
      const input = state.taskInputs[action.taskId];
      if (!action.message) return state;
      return {
        ...state,
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: {
            ...(input ?? { prompt: "", context: [] }),
            error: action.message,
          },
        },
      };
    }
    case "taskInput:cancelError": {
      const input = state.taskInputs[action.taskId] ?? { prompt: "", context: [] };
      return {
        ...state,
        taskInputs: {
          ...state.taskInputs,
          [action.taskId]: { ...input, error: action.message },
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
            ...acceptedInputIdentity(input),
            error: action.message,
          },
        },
      };
    }
    case "taskOpen:start":
      if (state.taskOpenError?.taskId !== action.taskId) return state;
      return { ...state, taskOpenError: undefined };
    case "taskOpen:error":
      if (
        (state.snapshot && state.snapshot.task.task_id !== action.taskId)
        || (state.activeTaskId !== undefined && state.activeTaskId !== action.taskId)
      ) return state;
      return { ...state, taskOpenError: { taskId: action.taskId, message: action.message } };
    case "chatPage:start": {
      const current = state.chatPages[action.taskId] ?? { olderItems: [], hasBefore: true };
      if ((current.requestGeneration ?? 0) >= action.requestGeneration) return state;
      return {
        ...state,
        chatPages: {
          ...state.chatPages,
          [action.taskId]: {
            ...current,
            requestGeneration: action.requestGeneration,
            pending: true,
            error: undefined,
          },
        },
      };
    }
    case "chatPage:result": {
      const current = state.chatPages[action.taskId];
      if (!current?.pending || current.requestGeneration !== action.requestGeneration) return state;
      return {
        ...state,
        chatPages: {
          ...state.chatPages,
          [action.taskId]: mergePageState(current, action.page),
        },
      };
    }
    case "chatPage:error": {
      const current = state.chatPages[action.taskId];
      if (!current?.pending || current.requestGeneration !== action.requestGeneration) return state;
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

function acceptedInputIdentity(input: AppState["taskInputs"][string] | undefined) {
  return input?.acceptedUserMessageId
    ? { acceptedUserMessageId: input.acceptedUserMessageId }
    : {};
}

function isTaskInteractionAction(action: AppAction): action is TaskInteractionAction {
  return action.type.startsWith("taskInput:")
    || action.type === "taskSend:accepted"
    || action.type === "taskOpen:start"
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
