import type {
  ActivityStatus as ProtocolActivityStatus,
  ActivityStepSnapshot,
  ChatItem,
  MessagePart,
  PendingRequestSnapshot,
  PermissionRequestOptionKind,
  PermissionRequestParams,
  ToolDetailSnapshot,
  TaskSnapshot as ProtocolTaskSnapshot,
} from "@openaide/app-server-client";
import type {
  ActivityStatus,
  ActivityStep,
  ActivityToolDetails,
  AgentMessagePart,
  Attachment,
  ChatMessage,
  NormalizedMessage,
} from "@openaide/app-shell-contracts";
import { mapPendingProtocolQuestion, mapProtocolQuestion } from "./questionProtocolMapping";

export function mapProtocolChatItem(item: ChatItem, createdAt: string): ChatMessage {
  const message = mapProtocolMessage(item, createdAt);
  return chatMessageFromProtocol(item.messageId, message);
}

export function pendingRequestItems(requests: PendingRequestSnapshot[], createdAt: string): ChatMessage[] {
  return requests.flatMap((request) => {
    if (request.scope.kind !== "task") return [];
    const permission = permissionRequestParams(request);
    if (request.kind === "permission" && permission) {
      return [permissionMessageFromPendingRequest(request, permission, createdAt)];
    }
    if (request.kind === "question" && request.question) {
      const messageId = `pending-${request.requestId}`;
      return [chatMessageFromProtocol(messageId, {
        ...mapPendingProtocolQuestion(request.requestId, request.question, createdAt),
        id: messageId,
      })];
    }
    return [];
  });
}

function permissionRequestParams(request: PendingRequestSnapshot): PermissionRequestParams | undefined {
  return request.permission ?? undefined;
}

function permissionMessageFromPendingRequest(
  request: PendingRequestSnapshot,
  params: PermissionRequestParams,
  createdAt: string,
): ChatMessage {
  const messageId = `pending-${request.requestId}`;
  return chatMessageFromProtocol(messageId, {
    kind: "permission",
    id: messageId,
    request_id: request.requestId,
    app_server_request_id: request.requestId,
    title: params.title,
    description: params.description ?? undefined,
    scope: params.scope ?? undefined,
    risk: params.risk ?? undefined,
    tool_call: {
      id: params.toolCall.id,
      title: params.toolCall.title,
      kind: params.toolCall.kind ?? undefined,
    },
    state: "pending",
    created_at: createdAt,
    options: params.options.map((option) => ({
      id: option.optionId,
      label: option.name,
      kind: permissionOptionKind(option.kind),
    })),
  });
}

function permissionOptionKind(kind: PermissionRequestOptionKind) {
  if (kind === "allowOnce" || kind === "allowAlways") return "allow";
  if (kind === "rejectOnce" || kind === "rejectAlways") return "deny";
  return "other";
}

export function recoveryItems(recovery: ProtocolTaskSnapshot["recovery"], createdAt: string): ChatMessage[] {
  if (!recovery) return [];
  return [
    chatMessageFromProtocol(
      "app-server-recovery",
      interruptionMessage("app-server-recovery", recovery.message, createdAt, recovery.actions.length > 0),
    ),
  ];
}

export function systemInterruptionItem(
  messageId: string,
  message: string,
  createdAt: string,
  recoverable: boolean,
): ChatMessage {
  return chatMessageFromProtocol(messageId, interruptionMessage(messageId, message, createdAt, recoverable));
}

function mapProtocolMessage(item: ChatItem, createdAt: string): NormalizedMessage {
  const text = textFromParts(item.parts);
  const activity = firstActivityPart(item.parts);
  if (activity) {
    return {
      kind: "activity",
      id: item.messageId,
      title: activity.title,
      status: activityStatusFromProtocol(activity.status),
      created_at: createdAt,
      collapsed: activity.status !== "running",
      steps: activitySteps(activity),
    };
  }
  if (item.status === "interrupted") {
    return interruptionMessage(item.messageId, text || "Task was interrupted.", createdAt, true);
  }

  if (item.role === "user") {
    return {
      kind: "user",
      id: item.messageId,
      text,
      created_at: createdAt,
      attachments: attachmentsFromParts(item.parts),
    };
  }

  const question = firstQuestionPart(item.parts);
  if (question) {
    return {
      ...mapProtocolQuestion(question, createdAt),
      id: item.messageId,
    };
  }

  const agentParts = agentMessageParts(item.parts);
  if (agentParts.length > 0) {
    return {
      kind: "agent_message",
      id: item.messageId,
      role: item.role === "system" ? "thought" : "agent",
      parts: agentParts,
      created_at: createdAt,
    };
  }

  return interruptionMessage(item.messageId, text || "Unsupported Chat message.", createdAt, false);
}

function interruptionMessage(id: string, message: string, createdAt: string, recoverable: boolean): NormalizedMessage {
  return {
    kind: "interruption",
    id,
    reason: "backend_unavailable",
    message,
    created_at: createdAt,
    recoverable,
  };
}

function chatMessageFromProtocol(messageId: string, message: NormalizedMessage): ChatMessage {
  return {
    cursor: messageId,
    identity: messageId,
    message_type: message.kind,
    message_id: messageId,
    message,
  };
}

function textFromParts(parts: MessagePart[]) {
  return parts
    .filter((part): part is Extract<MessagePart, { kind: "text" }> => part.kind === "text")
    .map((part) => part.text)
    .join("");
}

function attachmentsFromParts(parts: MessagePart[]): Attachment[] | undefined {
  const attachments = parts.flatMap((part): Attachment[] => {
    if (part.kind === "attachment") {
      return [{
        id: part.attachment.attachmentId,
        kind: "file",
        label: part.attachment.label,
        payload: attachmentPayload(part.attachment),
      }];
    }
    if (part.kind === "image") {
      return [{
        kind: "image",
        label: "Image",
        payload: {
          previewUrl: part.dataUrl,
          mimeType: part.mediaType,
        },
      }];
    }
    return [];
  });
  return attachments.length ? attachments : undefined;
}

function attachmentPayload(attachment: Extract<MessagePart, { kind: "attachment" }>["attachment"]): Attachment["payload"] {
  if (!attachment.previewUrl && !attachment.mediaType && attachment.sizeBytes === undefined) return undefined;
  return {
    previewUrl: attachment.previewUrl,
    mimeType: attachment.mediaType ?? undefined,
    sizeBytes: attachment.sizeBytes ?? undefined,
  };
}

function firstActivityPart(parts: MessagePart[]) {
  return parts.find((part): part is Extract<MessagePart, { kind: "activity" }> => part.kind === "activity");
}

function agentMessageParts(parts: MessagePart[]): AgentMessagePart[] {
  return parts.flatMap((part): AgentMessagePart[] => {
    if (part.kind === "text") return [{ kind: "text", text: part.text }];
    if (part.kind === "image") {
      return [{
        kind: "image",
        media_type: part.mediaType,
        data_url: part.dataUrl,
        uri: part.uri ?? undefined,
      }];
    }
    if (part.kind === "resource") {
      return [{
        kind: "resource",
        uri: part.uri,
        name: part.name ?? undefined,
        title: part.title ?? undefined,
        description: part.description ?? undefined,
        media_type: part.mediaType ?? undefined,
        size_bytes: part.sizeBytes ?? undefined,
        text: part.text ?? undefined,
      }];
    }
    if (part.kind === "unsupported") {
      return [{
        kind: "unsupported",
        content_type: part.contentType,
        media_type: part.mediaType ?? undefined,
        uri: part.uri ?? undefined,
      }];
    }
    return [];
  });
}

function firstQuestionPart(parts: MessagePart[]) {
  return parts.find((part): part is Extract<MessagePart, { kind: "question" }> => part.kind === "question");
}

function activitySteps(activity: Extract<MessagePart, { kind: "activity" }>): ActivityStep[] {
  if (activity.steps?.length) return activity.steps.map((step) => activityStepFromProtocol(step, activity.title));
  return [
    {
      kind: "text",
      text: activity.title,
      level: activity.status === "failed" ? "error" : "info",
    },
  ];
}

function activityStepFromProtocol(step: ActivityStepSnapshot, activityTitle: string): ActivityStep {
  if (step.kind === "text") {
    return {
      kind: "text",
      text: step.text,
      level: activityStepLevel(step.level),
    };
  }
  if (step.kind === "command") {
    return {
      kind: "command",
      command_label: step.commandLabel,
      status: activityStatusFromProtocol(step.status),
      exit_code: step.exitCode ?? undefined,
      output_preview: step.outputPreview ?? undefined,
    };
  }
  return {
    kind: "tool",
    tool_call_id: step.toolCallId ?? undefined,
    name: step.name,
    status: activityStatusFromProtocol(step.status),
    input_summary: step.inputSummary ?? activityTitle,
    output_preview: step.outputPreview ?? undefined,
    detail_artifact_id: step.detailArtifactId ?? undefined,
    details: step.details ? mapProtocolToolDetail(step.details) : undefined,
    permission_outcomes: (step.permissionOutcomes ?? []).map((outcome) => ({
      request_id: outcome.requestId,
      decision: outcome.decision,
      option_id: outcome.optionId ?? undefined,
      option_label: outcome.optionLabel ?? undefined,
      resolved_at: outcome.resolvedAt,
    })),
  };
}

function activityStatusFromProtocol(status: ProtocolActivityStatus): ActivityStatus {
  return status === "failed" ? "error" : status;
}

function activityStepLevel(level: string | null | undefined): Extract<ActivityStep, { kind: "text" }>["level"] {
  if (level === "info" || level === "warning" || level === "error") return level;
  return undefined;
}

export function mapProtocolToolDetail(details: ToolDetailSnapshot): ActivityToolDetails {
  return {
    locations: details.locations.map((location) => ({
      path: location.path,
      line: location.line ?? undefined,
    })),
    content: details.content.map((content) => {
      if (content.kind === "text") return content;
      if (content.kind === "diff") {
        return {
          kind: "diff" as const,
          path: content.path,
          old_text: content.oldText ?? undefined,
          new_text: content.newText,
        };
      }
      if (content.kind === "terminal") return { kind: "terminal" as const, terminal_id: content.terminalId };
      if (content.kind === "image") {
        return {
          kind: "image" as const,
          media_type: content.mediaType,
          data_url: content.dataUrl,
          uri: content.uri ?? undefined,
        };
      }
      if (content.kind === "audio") {
        return { kind: "audio" as const, media_type: content.mediaType, data_url: content.dataUrl };
      }
      if (content.kind === "resource") {
        return {
          kind: "resource" as const,
          uri: content.uri,
          name: content.name ?? undefined,
          title: content.title ?? undefined,
          description: content.description ?? undefined,
          media_type: content.mediaType ?? undefined,
          size_bytes: content.sizeBytes ?? undefined,
          text: content.text ?? undefined,
        };
      }
      return {
        kind: "unsupported" as const,
        content_type: content.contentType,
        media_type: content.mediaType ?? undefined,
        uri: content.uri ?? undefined,
      };
    }),
    input: details.input
      ? {
          command: details.input.command,
          cwd: details.input.cwd ?? undefined,
          query: details.input.query ?? undefined,
          queries: details.input.queries ?? undefined,
          url: details.input.url ?? undefined,
          path: details.input.path ?? undefined,
          fields: details.input.fields,
        }
      : undefined,
    output: details.output
      ? {
          stdout: details.output.stdout ?? undefined,
          stderr: details.output.stderr ?? undefined,
          formatted_output: details.output.formattedOutput ?? undefined,
          aggregated_output: details.output.aggregatedOutput ?? undefined,
          exit_code: details.output.exitCode ?? undefined,
          success: details.output.success ?? undefined,
          fields: details.output.fields,
        }
      : undefined,
  };
}
