import type { ElicitationField, ElicitationValue } from "@openaide/app-shell-contracts";
import { useId } from "react";

export function QuestionField({
  disabled,
  error,
  field,
  onChange,
  value,
}: {
  disabled: boolean;
  error?: string;
  field: ElicitationField;
  onChange: (value: ElicitationValue | undefined) => void;
  value: ElicitationValue | undefined;
}) {
  const id = useId();
  const errorId = `${id}-error`;
  const descriptionId = field.description ? `${id}-description` : undefined;
  const describedBy = [descriptionId, error ? errorId : undefined].filter(Boolean).join(" ") || undefined;
  const label = <QuestionFieldLabel descriptionId={descriptionId} field={field} />;
  if (field.kind === "singleSelect" || field.kind === "multiSelect" || field.kind === "boolean") {
    return (
      <fieldset className={`question-field question-choice-field${error ? " invalid" : ""}`} disabled={disabled}>
        <legend>{label}</legend>
        {field.kind === "boolean" ? (
          <BooleanChoices describedBy={describedBy} field={field} onChange={onChange} value={value} />
        ) : (
          <SelectionControl describedBy={describedBy} field={field} onChange={onChange} value={value} />
        )}
        {error ? <p className="question-field-error" id={errorId} role="alert">{error}</p> : null}
      </fieldset>
    );
  }
  return (
    <label className={`question-field question-value-field${error ? " invalid" : ""}`}>
      {label}
      {field.kind === "string" && field.format === "multiline" ? (
        <textarea
          aria-describedby={describedBy}
          aria-invalid={Boolean(error)}
          aria-label={field.label}
          disabled={disabled}
          maxLength={field.max_length}
          minLength={field.min_length}
          onChange={(event) => onChange(event.currentTarget.value)}
          required={field.required}
          rows={3}
          value={typeof value === "string" ? value : ""}
        />
      ) : (
        <input
          aria-describedby={describedBy}
          aria-invalid={Boolean(error)}
          aria-label={field.label}
          disabled={disabled}
          max={field.kind === "string" ? undefined : field.maximum}
          maxLength={field.kind === "string" ? field.max_length : undefined}
          min={field.kind === "string" ? undefined : field.minimum}
          minLength={field.kind === "string" ? field.min_length : undefined}
          onChange={(event) => onChange(inputValue(field, event.currentTarget.value))}
          pattern={field.kind === "string" ? field.pattern : undefined}
          required={field.required}
          step={field.kind === "integer" ? 1 : field.kind === "number" ? "any" : undefined}
          type={field.kind === "string" ? inputType(field.format) : "number"}
          value={typeof value === "string" || typeof value === "number" ? value : ""}
        />
      )}
      {error ? <span className="question-field-error" id={errorId} role="alert">{error}</span> : null}
    </label>
  );
}

function QuestionFieldLabel({
  descriptionId,
  field,
}: {
  descriptionId?: string;
  field: ElicitationField;
}) {
  return (
    <span className="question-field-label">
      <strong>{field.label}{field.required ? <span className="question-required" aria-label="required"> *</span> : null}</strong>
      {field.description ? <small id={descriptionId}>{field.description}</small> : null}
    </span>
  );
}

function BooleanChoices({
  describedBy,
  field,
  onChange,
  value,
}: {
  describedBy?: string;
  field: Extract<ElicitationField, { kind: "boolean" }>;
  onChange: (value: ElicitationValue) => void;
  value: ElicitationValue | undefined;
}) {
  return (
    <div className="question-choice-list compact" aria-describedby={describedBy}>
      {[{ label: "Yes", value: true }, { label: "No", value: false }].map((option) => (
        <label key={option.label}>
          <input
            checked={value === option.value}
            name={field.id}
            onChange={() => onChange(option.value)}
            required={field.required}
            type="radio"
          />
          <span>{option.label}</span>
        </label>
      ))}
    </div>
  );
}

function SelectionControl({
  describedBy,
  field,
  onChange,
  value,
}: {
  describedBy?: string;
  field: Extract<ElicitationField, { kind: "singleSelect" | "multiSelect" }>;
  onChange: (value: ElicitationValue | undefined) => void;
  value: ElicitationValue | undefined;
}) {
  if (field.options.length > 5) {
    const selected = field.kind === "multiSelect" && Array.isArray(value) ? value : typeof value === "string" ? value : "";
    return (
      <select
        aria-describedby={describedBy}
        aria-label={field.label}
        multiple={field.kind === "multiSelect"}
        onChange={(event) => onChange(
          field.kind === "multiSelect"
            ? Array.from(event.currentTarget.selectedOptions, (option) => option.value)
            : event.currentTarget.value || undefined,
        )}
        required={field.required}
        value={selected}
      >
        {field.kind === "singleSelect" ? <option value="">Choose an option</option> : null}
        {field.options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
      </select>
    );
  }
  const selected = Array.isArray(value) ? value : [];
  return (
    <div className="question-choice-list" aria-describedby={describedBy}>
      {field.options.map((option) => (
        <label key={option.value}>
          <input
            checked={field.kind === "multiSelect" ? selected.includes(option.value) : value === option.value}
            name={field.id}
            onChange={(event) => onChange(
              field.kind === "multiSelect"
                ? toggleSelection(selected, option.value, event.currentTarget.checked)
                : option.value,
            )}
            required={field.required && field.kind === "singleSelect"}
            type={field.kind === "multiSelect" ? "checkbox" : "radio"}
          />
          <span>
            <strong>{option.label}</strong>
            {option.description ? <small>{option.description}</small> : null}
          </span>
        </label>
      ))}
    </div>
  );
}

function toggleSelection(current: string[], value: string, selected: boolean) {
  return selected ? [...current, value] : current.filter((item) => item !== value);
}

function inputValue(field: ElicitationField, value: string): ElicitationValue | undefined {
  if (field.kind === "string") return value;
  if (!value) return undefined;
  return Number(value);
}

function inputType(format: Extract<ElicitationField, { kind: "string" }>["format"]) {
  if (format === "email" || format === "date" || format === "date-time") {
    return format === "date-time" ? "datetime-local" : format;
  }
  if (format === "uri") return "url";
  return "text";
}
