import type { DiagnosticsSnapshot, RuntimeDiagnostics } from "@openaide/app-shell-contracts";
import {
  DIAGNOSTICS_GET_RUNTIME,
  type RuntimeDiagnosticsResult,
} from "@openaide/app-server-client";
import { sanitizeDiagnosticText } from "../logging/logger";
import { RuntimeProcess } from "../runtime/process";
import { RuntimeClient } from "../runtime/rpcClient";

export async function collectDiagnostics(
  runtime: Pick<RuntimeClient, "appServerRequest">,
  runtimeProcess: Pick<RuntimeProcess, "describe">,
): Promise<DiagnosticsSnapshot> {
  const runtimeResult = await collectRuntimeDiagnostics(runtime);
  return {
    created_at: new Date().toISOString(),
    runtime: runtimeResult.diagnostics,
    notices: runtimeResult.notice ? [runtimeResult.notice] : [],
    process: runtimeProcess.describe(),
  };
}

export async function collectRuntimeDiagnostics(runtime: Pick<RuntimeClient, "appServerRequest">): Promise<{
  diagnostics: RuntimeDiagnostics;
  notice?: DiagnosticsSnapshot["notices"][number];
}> {
  try {
    return {
      diagnostics: allowlistedRuntimeDiagnostics(
        toShellRuntimeDiagnostics(await runtime.appServerRequest(DIAGNOSTICS_GET_RUNTIME, {})),
      ),
    };
  } catch (error) {
    return {
      diagnostics: {
        status: "degraded",
        method_count: 0,
        tasks: {
          visible_count: 0,
          total_count: 0,
          active_count: 0,
          active_tasks: [],
          revision: 0,
        },
        redaction: "prompt_text_file_contents_terminal_output_and_secrets_removed",
      },
      notice: {
        component: "runtime",
        severity: "error",
        message: sanitizeDiagnosticText(error),
      },
    };
  }
}

function toShellRuntimeDiagnostics(result: RuntimeDiagnosticsResult): RuntimeDiagnostics {
  return {
    status: result.status,
    version: result.version ?? undefined,
    method_count: result.methodCount,
    tasks: {
      visible_count: result.tasks.visibleCount,
      total_count: result.tasks.totalCount,
      active_count: result.tasks.activeCount,
      active_tasks: (result.tasks.activeTasks ?? []).map((task) => ({
        task_id: task.taskId,
        agent_id: task.agentId,
        status: task.status,
        updated_at: task.updatedAt,
        last_activity: task.lastActivity,
        active_turn_id: task.activeTurnId ?? undefined,
        has_agent_session: task.hasAgentSession,
      })),
      revision: result.tasks.revision,
    },
    redaction: result.redaction,
  };
}

export function allowlistedRuntimeDiagnostics(value: unknown): RuntimeDiagnostics {
  const input = isRecord(value) ? value : {};
  const tasks = isRecord(input.tasks) ? input.tasks : {};
  return {
    status: input.status === "ready" ? "ready" : "degraded",
    version: typeof input.version === "string" ? sanitizeDiagnosticText(input.version) : undefined,
    method_count: toSafeNumber(input.method_count),
    tasks: {
      visible_count: toSafeNumber(tasks.visible_count),
      total_count: toSafeNumber(tasks.total_count),
      active_count: toSafeNumber(tasks.active_count),
      active_tasks: Array.isArray(tasks.active_tasks)
        ? tasks.active_tasks.map(allowlistedActiveTaskDiagnostics).filter((task) => task !== undefined)
        : [],
      revision: toSafeNumber(tasks.revision),
    },
    redaction: "prompt_text_file_contents_terminal_output_and_secrets_removed",
  };
}

function allowlistedActiveTaskDiagnostics(value: unknown): RuntimeDiagnostics["tasks"]["active_tasks"][number] | undefined {
  if (!isRecord(value)) return undefined;
  const taskId = safeText(value.task_id);
  const agentId = safeText(value.agent_id);
  const status = activeTaskStatus(value.status);
  const updatedAt = safeText(value.updated_at);
  const lastActivity = safeText(value.last_activity);
  if (!taskId || !agentId || !status || !updatedAt || !lastActivity) return undefined;
  return {
    task_id: taskId,
    agent_id: agentId,
    status,
    updated_at: updatedAt,
    last_activity: lastActivity,
    active_turn_id: safeText(value.active_turn_id) || undefined,
    has_agent_session: value.has_agent_session === true,
  };
}

function safeText(value: unknown) {
  return typeof value === "string" ? sanitizeDiagnosticText(value) : "";
}

function activeTaskStatus(value: unknown): RuntimeDiagnostics["tasks"]["active_tasks"][number]["status"] | undefined {
  switch (value) {
    case "preparing":
    case "starting":
    case "idle":
    case "running":
    case "blocked":
    case "interrupted":
    case "failed":
    case "completed":
      return value;
    default:
      return undefined;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function toSafeNumber(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : 0;
}
