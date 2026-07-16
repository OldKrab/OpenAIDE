import { act, create } from "react-test-renderer";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { ElicitationMessage } from "@openaide/app-shell-contracts";
import { QuestionCard } from "./QuestionCard";
import { initialQuestionValues, validateQuestionValues } from "./questionFormModel";

describe("QuestionCard", () => {
  it("validates the supported normalized field types before submit", () => {
    const question = pendingQuestion();
    const values = initialQuestionValues(question.fields);

    expect(values).toEqual({ enabled: true, name: "Question", scope: "form", tags: ["ui"] });
    expect(validateQuestionValues(question.fields, { ...values, count: 2 })).toEqual({});
    expect(validateQuestionValues(question.fields, {
      ...values,
      count: 1.5,
      name: "Q",
      scope: "unknown",
      tags: [],
    })).toEqual({
      count: "Enter a whole number.",
      name: "Enter at least 3 characters.",
      scope: "Choose a valid option.",
      tags: "Choose at least one option.",
    });
  });

  it("renders one quiet form surface with unnumbered separated fields and two actions", () => {
    const html = renderToStaticMarkup(<QuestionCard elicitation={pendingQuestion()} onRespond={vi.fn()} />);

    expect(html).toContain('aria-label="Question"');
    expect(html).toContain('class="question-field');
    expect(html).toContain("Question needs your input to continue.");
    expect(html).toContain(">Submit<");
    expect(html).toContain(">Cancel<");
    expect(html).not.toMatch(/question-number|>1\.<|>2\.</);
  });

  it("submits typed values together and cancels without confirmation", () => {
    const onRespond = vi.fn();
    let tree: ReturnType<typeof create>;
    act(() => { tree = create(<QuestionCard elicitation={pendingQuestion()} onRespond={onRespond} />); });
    const root = tree!.root;

    act(() => root.findByProps({ "aria-label": "Count" }).props.onChange({ currentTarget: { value: "2" } }));
    act(() => root.findByType("form").props.onSubmit({ preventDefault: vi.fn() }));
    expect(onRespond).toHaveBeenNthCalledWith(1, "server-question-1", {
      action: "submit",
      content: { count: 2, enabled: true, name: "Question", scope: "form", tags: ["ui"] },
    });

    act(() => root.findByProps({ children: "Cancel" }).props.onClick());
    expect(onRespond).toHaveBeenNthCalledWith(2, "server-question-1", { action: "cancel" });
  });

  it("disables the whole form while responding and exposes a recoverable response error", () => {
    const html = renderToStaticMarkup(
      <QuestionCard
        elicitation={pendingQuestion()}
        onRespond={vi.fn()}
        response={{ responding: true, error: "Response was rejected." }}
      />,
    );

    expect(html).toContain('aria-busy="true"');
    expect(html).toContain("Submitting");
    expect(html).toContain("Response was rejected.");
    expect(html).toContain("disabled");
  });

  it("previews three saved answers and expands the remainder", () => {
    let tree: ReturnType<typeof create>;
    act(() => { tree = create(<QuestionCard elicitation={resolvedQuestion()} onRespond={vi.fn()} />); });
    const compact = JSON.stringify(tree!.toJSON());
    expect(compact).toContain("Question answered");
    expect(tree!.root.findByProps({ children: "4 answers" })).toBeDefined();
    expect(compact).not.toContain("submitted");
    expect(tree!.root.findByProps({ "aria-label": "Question answered" }).findByType("header")).toBeDefined();
    expect(tree!.root.findByType("dl").children.map((child) => (
      typeof child === "string" ? child : child.type
    ))).toEqual(["dt", "dd", "dt", "dd", "dt", "dd"]);
    expect(compact).toContain("Form only");
    expect(compact).not.toContain("No confirmation");

    act(() => tree!.root.findByProps({ children: "Show all" }).props.onClick());
    expect(JSON.stringify(tree!.toJSON())).toContain("No confirmation");
    expect(tree!.root.findByProps({ children: "Show less" })).toBeDefined();
  });

  it("keeps cancelled and failed questions as compact history blocks", () => {
    const cancelled = renderToStaticMarkup(
      <QuestionCard elicitation={{ ...pendingQuestion(), state: "cancelled" }} onRespond={vi.fn()} />,
    );
    const failed = renderToStaticMarkup(
      <QuestionCard elicitation={{ ...pendingQuestion(), state: "error" }} onRespond={vi.fn()} />,
    );
    expect(cancelled).toContain("Question closed");
    expect(cancelled).toContain("Closed without response");
    expect(renderToStaticMarkup(
      <QuestionCard
        elicitation={{
          ...pendingQuestion(),
          state: "cancelled",
          resolution_message: "Task stopped while a question was pending.",
        }}
        onRespond={vi.fn()}
      />,
    )).toContain("Task stopped while a question was pending.");
    expect(failed).toContain("Question unavailable");
  });
});

function pendingQuestion(): ElicitationMessage {
  return {
    kind: "elicitation",
    id: "question-1",
    request_id: "question-1",
    app_server_request_id: "server-question-1",
    prompt: "Question needs your input to continue.",
    state: "pending",
    created_at: "2026-07-10T00:00:00Z",
    fields: [
      { id: "scope", kind: "singleSelect", label: "Implementation scope", required: true, default_value: "form", options: [
        { value: "form", label: "Form only", description: "Support structured questions first." },
        { value: "both", label: "Form and URL" },
      ] },
      { id: "name", kind: "string", label: "Interaction name", required: true, default_value: "Question", min_length: 3 },
      { id: "count", kind: "integer", label: "Count", required: true, minimum: 1, maximum: 3 },
      { id: "enabled", kind: "boolean", label: "Enable it", required: true, default_value: true },
      { id: "tags", kind: "multiSelect", label: "Areas", required: true, default_value: ["ui"], options: [
        { value: "ui", label: "UI" },
        { value: "runtime", label: "Runtime" },
      ] },
    ],
  };
}

function resolvedQuestion(): ElicitationMessage {
  return {
    ...pendingQuestion(),
    state: "resolved",
    answers: [
      { field_id: "scope", label: "Implementation scope", value: "Form only" },
      { field_id: "name", label: "Interaction name", value: "Question" },
      { field_id: "count", label: "Preview count", value: 3 },
      { field_id: "cancel", label: "Cancel confirmation", value: "No confirmation" },
    ],
  };
}
