import { describe, expect, it } from "vitest";

import { webviewTelemetryFields } from "./messagingFields";

describe("webview telemetry fields", () => {
  it("keeps New Task selection evidence without accepting free-form content", () => {
    expect(webviewTelemetryFields({
      event: "new_task_initial_project_selected",
      project_id: "project-safe",
      selection_source: "app_server_default",
      client_identity_source: "shell",
      retained_project_present: true,
      default_project_valid: true,
      prompt_text: "private task content",
    })).toEqual({
      event: "new_task_initial_project_selected",
      project_id: "project-safe",
      selection_source: "app_server_default",
      client_identity_source: "shell",
      retained_project_present: true,
      default_project_valid: true,
    });
  });
});
