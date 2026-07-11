import { describe, expect, it } from "vitest";
import type { RequestId } from "@openaide/app-server-client";
import { mapProtocolQuestion } from "./questionProtocolMapping";

describe("Question protocol mapping", () => {
  it("normalizes every form field without exposing protocol field names to the view", () => {
    const message = mapProtocolQuestion({
      kind: "question",
      requestId: "question-1" as RequestId,
      message: "Choose values.",
      state: "pending",
      fields: [
        { kind: "string", key: "name", title: "Name", description: "Visible label", required: true, default: "Question", minLength: 3, maxLength: 20, pattern: "^[A-Z]", format: "email" },
        { kind: "number", key: "weight", title: "Weight", required: false, default: 1.5, minimum: 0, maximum: 2 },
        { kind: "integer", key: "count", title: "Count", required: true, minimum: 1, maximum: 3 },
        { kind: "boolean", key: "enabled", title: "Enabled", required: true, default: true },
        { kind: "singleSelect", key: "scope", title: "Scope", required: true, default: "form", options: [{ value: "form", label: "Form only" }] },
        { kind: "multiSelect", key: "areas", title: "Areas", required: false, default: ["ui"], minItems: 1, maxItems: 2, options: [{ value: "ui", label: "UI" }] },
      ],
    }, "2026-07-10T00:00:00Z", "server-question-1");

    expect(message.kind).toBe("elicitation");
    expect(message.app_server_request_id).toBe("server-question-1");
    expect(message.fields.map((field) => field.kind)).toEqual([
      "string", "number", "integer", "boolean", "singleSelect", "multiSelect",
    ]);
    expect(message.fields[0]).toMatchObject({
      id: "name", label: "Name", default_value: "Question", min_length: 3, max_length: 20,
    });
    expect(message.fields[5]).toMatchObject({ default_value: ["ui"], min_items: 1, max_items: 2 });
  });

  it("builds durable question-and-answer history with human-readable choice labels", () => {
    const message = mapProtocolQuestion({
      kind: "question",
      requestId: "question-1" as RequestId,
      message: "Choose values.",
      state: "resolved",
      action: "submit",
      fields: [
        { kind: "singleSelect", key: "scope", title: "Scope", required: true, options: [{ value: "form", label: "Form only" }] },
        { kind: "multiSelect", key: "areas", title: "Areas", required: true, options: [{ value: "ui", label: "UI" }, { value: "runtime", label: "Runtime" }] },
      ],
      content: { scope: "form", areas: ["ui", "runtime"] },
    }, "2026-07-10T00:00:00Z");

    expect(message.answers).toEqual([
      { field_id: "scope", label: "Scope", value: "Form only" },
      { field_id: "areas", label: "Areas", value: ["UI", "Runtime"] },
    ]);
  });
});
