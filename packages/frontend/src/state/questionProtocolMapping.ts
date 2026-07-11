import type {
  ElicitationAnswer,
  ElicitationField,
  ElicitationMessage,
  ElicitationValue,
} from "@openaide/app-shell-contracts";
import type { MessagePart, QuestionField, QuestionOption, QuestionRequestParams, RequestId } from "@openaide/app-server-client";

type ProtocolQuestionMessage = Extract<MessagePart, { kind: "question" }>;

export function mapProtocolQuestion(
  question: ProtocolQuestionMessage,
  createdAt: string,
  appServerRequestId: string = question.requestId,
): ElicitationMessage {
  const fields = question.fields.map(mapProtocolQuestionField);
  return {
    kind: "elicitation",
    id: question.requestId,
    request_id: question.requestId,
    app_server_request_id: appServerRequestId,
    prompt: question.message,
    state: question.state,
    created_at: createdAt,
    fields,
    answers: question.action === "submit" ? questionAnswers(question.fields, question.content) : undefined,
    error: question.error ?? undefined,
  };
}

export function mapPendingProtocolQuestion(
  requestId: RequestId,
  question: QuestionRequestParams,
  createdAt: string,
) {
  return mapProtocolQuestion({
    kind: "question",
    requestId,
    message: question.message,
    fields: question.fields,
    state: "pending",
  }, createdAt, requestId);
}

export function mapProtocolQuestionField(field: QuestionField): ElicitationField {
  const base = {
    id: field.key,
    label: field.title,
    description: field.description ?? undefined,
    required: field.required,
  };
  if (field.kind === "string") {
    return {
      ...base,
      kind: "string",
      default_value: field.default ?? undefined,
      min_length: field.minLength ?? undefined,
      max_length: field.maxLength ?? undefined,
      pattern: field.pattern ?? undefined,
      format: field.format ?? undefined,
    };
  }
  if (field.kind === "number" || field.kind === "integer") {
    return {
      ...base,
      kind: field.kind,
      default_value: field.default ?? undefined,
      minimum: field.minimum ?? undefined,
      maximum: field.maximum ?? undefined,
    };
  }
  if (field.kind === "boolean") {
    return { ...base, kind: "boolean", default_value: field.default ?? undefined };
  }
  if (field.kind === "singleSelect") {
    return { ...base, kind: "singleSelect", default_value: field.default ?? undefined, options: mapOptions(field.options) };
  }
  return {
    ...base,
    kind: "multiSelect",
    default_value: field.default ?? undefined,
    min_items: field.minItems ?? undefined,
    max_items: field.maxItems ?? undefined,
    options: mapOptions(field.options),
  };
}

function mapOptions(options: QuestionOption[]) {
  return options.map((option) => ({
    value: option.value,
    label: option.label,
    description: option.description ?? undefined,
  }));
}

function questionAnswers(
  fields: QuestionField[],
  content: Record<string, ElicitationValue> | null | undefined,
): ElicitationAnswer[] {
  if (!content) return [];
  return fields.flatMap((field) => {
    const value = content[field.key];
    if (value === undefined) return [];
    return [{ field_id: field.key, label: field.title, value: answerValue(field, value) }];
  });
}

function answerValue(field: QuestionField, value: ElicitationValue): ElicitationValue {
  if (field.kind !== "singleSelect" && field.kind !== "multiSelect") return value;
  const labels = new Map(field.options.map((option) => [option.value, option.label]));
  if (Array.isArray(value)) return value.map((item) => labels.get(item) ?? item);
  return typeof value === "string" ? (labels.get(value) ?? value) : value;
}
