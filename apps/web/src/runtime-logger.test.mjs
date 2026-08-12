import assert from "node:assert/strict";
import test from "node:test";
import { createRuntimeLogger, safeWebviewTelemetryFields } from "./runtime-logger.mjs";

test("web runtime logger fully redacts sensitive fields", () => {
  const lines = [];
  const logger = createRuntimeLogger("test", {
    info: (line) => lines.push(line),
    warn: (line) => lines.push(line),
    error: (line) => lines.push(line),
  });

  logger.warn("handoff_failed", {
    error: "arbitrary-private-detail",
    endpointUrl: "http://example.invalid/private",
    error_kind: "transport_closed",
    error_name: "TypeError",
    attempt_count: 2,
  });

  assert.equal(lines.length, 1);
  assert.doesNotMatch(lines[0], /arbitrary-private-detail|example\.invalid/);
  assert.match(lines[0], /transport_closed/);
  assert.match(lines[0], /TypeError/);
  assert.match(lines[0], /"attempt_count":2/);
});

test("webview telemetry keeps correlation metadata and drops arbitrary detail", () => {
  assert.deepEqual(safeWebviewTelemetryFields({
    event: "app_server_initialize_failed",
    error_name: "TypeError",
    task_id: "task_123",
    error_message: "Cannot read /private/workspace",
    unexpected: "private payload",
  }), {
    event: "app_server_initialize_failed",
    error_name: "TypeError",
    task_id: "task_123",
  });
});

test("webview telemetry keeps Diagram lifecycle outcomes without source content", () => {
  assert.deepEqual(safeWebviewTelemetryFields({
    event: "diagram_render_terminal",
    operation_id: "diagram-operation-1",
    attempt: 1,
    duration_ms: 30_000,
    queue_depth: 0,
    source_length: 320,
    output_bytes: 12_000,
    outcome: "startup_timeout",
    cache_hit: false,
    source: "flowchart LR; private --> content",
  }), {
    event: "diagram_render_terminal",
    operation_id: "diagram-operation-1",
    attempt: 1,
    duration_ms: 30_000,
    queue_depth: 0,
    source_length: 320,
    output_bytes: 12_000,
    outcome: "startup_timeout",
    cache_hit: false,
  });
});
