import type { ElicitationField, ElicitationValue } from "@openaide/app-shell-contracts";

export type QuestionValues = Partial<Record<string, ElicitationValue>>;
export type QuestionErrors = Record<string, string>;

export function initialQuestionValues(fields: ElicitationField[]): QuestionValues {
  return Object.fromEntries(
    fields.flatMap((field) => field.default_value === undefined ? [] : [[field.id, field.default_value]]),
  );
}

export function validateQuestionValues(fields: ElicitationField[], values: QuestionValues): QuestionErrors {
  const errors: QuestionErrors = {};
  for (const field of fields) {
    const error = validateQuestionValue(field, values[field.id]);
    if (error) errors[field.id] = error;
  }
  return errors;
}

export function submittedQuestionValues(values: QuestionValues) {
  return Object.fromEntries(
    Object.entries(values).filter((entry): entry is [string, ElicitationValue] => entry[1] !== undefined),
  );
}

function validateQuestionValue(field: ElicitationField, value: ElicitationValue | undefined) {
  if (isEmpty(value)) return field.required ? requiredMessage(field) : undefined;
  if (value === undefined) return undefined;
  if (field.kind === "string") return validateString(field, value);
  if (field.kind === "number" || field.kind === "integer") return validateNumber(field, value);
  if (field.kind === "boolean") return typeof value === "boolean" ? undefined : "Choose Yes or No.";
  return validateSelection(field, value);
}

function validateString(field: Extract<ElicitationField, { kind: "string" }>, value: ElicitationValue) {
  if (typeof value !== "string") return "Enter a value.";
  if (field.min_length !== undefined && value.length < field.min_length) {
    return `Enter at least ${field.min_length} characters.`;
  }
  if (field.max_length !== undefined && value.length > field.max_length) {
    return `Enter no more than ${field.max_length} characters.`;
  }
  if (field.pattern && !matchesPattern(value, field.pattern)) return "Enter a value in the requested format.";
  if (field.format === "email" && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value)) return "Enter an email address.";
  if (field.format === "uri" && !isAbsoluteUrl(value)) return "Enter a complete URL.";
  return undefined;
}

function isAbsoluteUrl(value: string) {
  try {
    return Boolean(new URL(value).protocol);
  } catch {
    return false;
  }
}

function matchesPattern(value: string, pattern: string) {
  try {
    return new RegExp(pattern, "u").test(value);
  } catch {
    // Invalid patterns are rejected by the App Server before reaching this surface.
    return false;
  }
}

function validateNumber(
  field: Extract<ElicitationField, { kind: "number" | "integer" }>,
  value: ElicitationValue,
) {
  if (typeof value !== "number" || !Number.isFinite(value)) return "Enter a number.";
  if (field.kind === "integer" && !Number.isInteger(value)) return "Enter a whole number.";
  if (field.minimum !== undefined && value < field.minimum) return `Enter ${field.minimum} or more.`;
  if (field.maximum !== undefined && value > field.maximum) return `Enter ${field.maximum} or less.`;
  return undefined;
}

function validateSelection(
  field: Extract<ElicitationField, { kind: "singleSelect" | "multiSelect" }>,
  value: ElicitationValue,
) {
  const allowed = new Set(field.options.map((option) => option.value));
  if (field.kind === "multiSelect") {
    if (!Array.isArray(value)) return "Choose at least one option.";
    if (field.required && value.length === 0) return "Choose at least one option.";
    if (field.min_items !== undefined && value.length < field.min_items) {
      return `Choose at least ${field.min_items} options.`;
    }
    if (field.max_items !== undefined && value.length > field.max_items) {
      return `Choose no more than ${field.max_items} options.`;
    }
    return value.every((item) => allowed.has(item)) ? undefined : "Choose valid options.";
  }
  return typeof value === "string" && allowed.has(value) ? undefined : "Choose a valid option.";
}

function requiredMessage(field: ElicitationField) {
  if (field.kind === "singleSelect") return "Choose an option.";
  if (field.kind === "multiSelect") return "Choose at least one option.";
  if (field.kind === "boolean") return "Choose Yes or No.";
  return "This field is required.";
}

function isEmpty(value: ElicitationValue | undefined) {
  return value === undefined || value === "" || (Array.isArray(value) && value.length === 0);
}
