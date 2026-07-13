import { describe, expect, it } from "vitest";
import { DIAGNOSTICS_GET_RUNTIME } from "@openaide/app-server-client";
import { allowlistedRuntimeDiagnostics, collectDiagnostics } from "./snapshot";

describe("diagnostics export", () => {
  it("preserves a Task that is stopping", () => {
    const diagnostics = allowlistedRuntimeDiagnostics({
      tasks: {
        active_tasks: [{
          task_id: "task-stopping",
          agent_id: "codex",
          status: "stopping",
          updated_at: "2026-07-06T10:00:00.000Z",
          last_activity: "2026-07-06T10:00:00.000Z",
          has_agent_session: true,
        }],
      },
    });

    expect(diagnostics.tasks.active_tasks[0]?.status).toBe("stopping");
  });

  it("allowlists runtime fields and strips sensitive extras", () => {
    const diagnostics = allowlistedRuntimeDiagnostics({
      status: "ready",
      version: "0.1.0 /workspace/private",
      method_count: 13,
      workspace_root: "/workspace/project",
      prompt_text: "secret prompt",
      tasks: {
        visible_count: 2,
        total_count: 3,
        active_count: 1,
        active_tasks: [
          {
            task_id: "task-1",
            agent_id: "codex",
            status: "running",
            updated_at: "2026-07-06T10:00:00.000Z",
            last_activity: "2026-07-06T10:00:00.000Z",
            active_turn_id: "turn-1",
            has_agent_session: true,
            native_session_id: "native-session-secret",
          },
        ],
        revision: 9,
        output: "terminal output",
      },
    });

    expect(diagnostics).toEqual({
      status: "ready",
      version: "0.1.0 [path]",
      method_count: 13,
      tasks: {
        visible_count: 2,
        total_count: 3,
        active_count: 1,
        active_tasks: [
          {
            task_id: "task-1",
            agent_id: "codex",
            status: "running",
            updated_at: "2026-07-06T10:00:00.000Z",
            last_activity: "2026-07-06T10:00:00.000Z",
            active_turn_id: "turn-1",
            has_agent_session: true,
          },
        ],
        revision: 9,
      },
      redaction: "prompt_text_file_contents_terminal_output_and_secrets_removed",
    });
    expect(JSON.stringify(diagnostics)).not.toContain("secret prompt");
    expect(JSON.stringify(diagnostics)).not.toContain("/workspace/project");
    expect(JSON.stringify(diagnostics)).not.toContain("native-session-secret");
  });

  it("returns degraded diagnostics when runtime diagnostics fails", async () => {
    const snapshot = await collectDiagnostics(
      {
        async appServerRequest() {
          throw new Error("spawn /workspace/private/runtime ENOENT");
        },
      },
      {
        describe() {
          return {
            running: false,
            runtime_source_kind: "development" as const,
            storage_root_kind: "extension-storage" as const,
          };
        },
      },
    );

    expect(snapshot.runtime.status).toBe("degraded");
    expect(snapshot.process.running).toBe(false);
    expect(snapshot.notices).toHaveLength(1);
    expect(snapshot.notices[0].message).not.toContain("/workspace/private");
  });

  it("captures process state after diagnostics call", async () => {
    let running = false;
    const snapshot = await collectDiagnostics(
      {
        async appServerRequest(method: string) {
          expect(method).toBe(DIAGNOSTICS_GET_RUNTIME);
          running = true;
          return {
            status: "ready",
            version: "0.1.0",
            methodCount: 13,
            tasks: {
              visibleCount: 0,
              totalCount: 0,
              activeCount: 0,
              activeTasks: [],
              revision: 0,
            },
            redaction: "prompt_text_file_contents_terminal_output_and_secrets_removed" as const,
          };
        },
      },
      {
        describe() {
          return {
            running,
            runtime_source_kind: "development" as const,
            storage_root_kind: "extension-storage" as const,
          };
        },
      },
    );

    expect(snapshot.runtime.status).toBe("ready");
    expect(snapshot.process.running).toBe(true);
  });
});
