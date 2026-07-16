import { AlertCircle, Check, CircleX, LoaderCircle } from "lucide-react";
import { Fragment, useState } from "react";
import type {
  ElicitationMessage,
  ElicitationResponse,
  ElicitationValue,
} from "@openaide/app-shell-contracts";
import { QuestionField } from "./QuestionField";
import { initialQuestionValues, submittedQuestionValues, validateQuestionValues, type QuestionValues } from "./questionFormModel";

export type QuestionResponseState = { responding: boolean; error?: string };

export function QuestionCard({
  elicitation,
  onRespond,
  response,
}: {
  elicitation: ElicitationMessage;
  onRespond: (requestId: string, response: ElicitationResponse) => void;
  response?: QuestionResponseState;
}) {
  if (elicitation.state !== "pending") return <ResolvedQuestion elicitation={elicitation} />;
  return <PendingQuestion elicitation={elicitation} onRespond={onRespond} response={response} />;
}

function PendingQuestion({
  elicitation,
  onRespond,
  response,
}: {
  elicitation: ElicitationMessage;
  onRespond: (requestId: string, response: ElicitationResponse) => void;
  response?: QuestionResponseState;
}) {
  const [values, setValues] = useState<QuestionValues>(() => initialQuestionValues(elicitation.fields));
  const [errors, setErrors] = useState<Record<string, string>>({});
  const responding = response?.responding ?? false;
  const requestId = elicitation.app_server_request_id ?? elicitation.request_id;
  const change = (fieldId: string, value: ElicitationValue | undefined) => {
    setValues((current) => ({ ...current, [fieldId]: value }));
    setErrors((current) => {
      if (!current[fieldId]) return current;
      const next = { ...current };
      delete next[fieldId];
      return next;
    });
  };
  const submit = () => {
    const nextErrors = validateQuestionValues(elicitation.fields, values);
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length) return;
    onRespond(requestId, { action: "submit", content: submittedQuestionValues(values) });
  };
  return (
    <form
      aria-busy={responding}
      aria-label="Question"
      className="question-card"
      noValidate
      onSubmit={(event) => { event.preventDefault(); submit(); }}
    >
      <QuestionHeader icon="pending" prompt={elicitation.prompt} status={responding ? "Responding" : "Waiting"} />
      <div className="question-fields">
        {elicitation.fields.map((field) => (
          <QuestionField
            disabled={responding}
            error={errors[field.id]}
            field={field}
            key={field.id}
            onChange={(value) => change(field.id, value)}
            value={values[field.id]}
          />
        ))}
      </div>
      <div className="question-actions">
        <button className="primary" disabled={responding} type="submit">
          {responding ? <><LoaderCircle aria-hidden="true" size={13} />Submitting</> : "Submit"}
        </button>
        <button disabled={responding} onClick={() => onRespond(requestId, { action: "cancel" })} type="button">
          Cancel
        </button>
      </div>
      {response?.error ? <p className="question-response-error" role="alert">{response.error}</p> : null}
    </form>
  );
}

function ResolvedQuestion({ elicitation }: { elicitation: ElicitationMessage }) {
  const [expanded, setExpanded] = useState(false);
  const answered = elicitation.state === "resolved";
  const failed = elicitation.state === "error";
  const answers = elicitation.answers ?? [];
  const shownAnswers = expanded ? answers : answers.slice(0, 3);
  const answerCount = `${answers.length} ${answers.length === 1 ? "answer" : "answers"}`;
  if (answered) {
    return (
      <section aria-label="Question answered" className="question-card question-card-resolved resolved">
        <QuestionHeader icon="answered" prompt={answerCount} status="Question answered" />
        {shownAnswers.length ? (
          <div className="question-answer-preview">
            <dl>
              {shownAnswers.map((answer) => (
                <Fragment key={answer.field_id}>
                  <dt>{answer.label}</dt>
                  <dd>{displayAnswer(answer.value)}</dd>
                </Fragment>
              ))}
            </dl>
            {answers.length > 3 ? (
              <button onClick={() => setExpanded((current) => !current)} type="button">
                {expanded ? "Show less" : "Show all"}
              </button>
            ) : null}
          </div>
        ) : null}
      </section>
    );
  }

  const title = failed ? "Question unavailable" : "Question closed";
  const detail = failed
    ? (elicitation.error ?? "The question could not be answered")
    : (elicitation.resolution_message ?? "Closed without response");
  return (
    <section aria-label={title} className={`question-card question-card-resolved ${elicitation.state}`}>
      <QuestionHeader icon={failed ? "error" : "closed"} prompt={detail} status={title} />
    </section>
  );
}

function QuestionHeader({
  icon,
  prompt,
  status,
}: {
  icon: "pending" | "answered" | "closed" | "error";
  prompt: string;
  status: string;
}) {
  return (
    <header className="question-heading">
      <QuestionSymbol icon={icon} />
      <span className="question-heading-copy">
        <strong>{status === "Waiting" || status === "Responding" ? "Question" : status}</strong>
        <small>{prompt}</small>
      </span>
      {(status === "Waiting" || status === "Responding") ? <span className="question-state">{status}</span> : null}
    </header>
  );
}

function QuestionSymbol({ icon }: { icon: "pending" | "answered" | "closed" | "error" }) {
  return (
    <span className={`question-symbol ${icon}`} aria-hidden="true">
      {icon === "answered" ? <Check size={13} /> : icon === "closed" ? <CircleX size={13} /> : icon === "error" ? <AlertCircle size={13} /> : "?"}
    </span>
  );
}

function displayAnswer(value: ElicitationValue) {
  if (Array.isArray(value)) return value.join(", ");
  if (typeof value === "boolean") return value ? "Yes" : "No";
  return String(value);
}
