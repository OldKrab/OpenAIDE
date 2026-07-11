import type { TaskStatus } from "../runtime/primitives.js";
import type { WebviewSurfaceKind } from "./bootstrap.js";

export type WebviewTelemetryPayload = {
  event?: string;
  surface?: WebviewSurfaceKind | "invalid";
  task_id?: string;
  snapshot_request_id?: number;
  latest_snapshot_request_id?: number;
  snapshot_intent?: "open" | "refresh";
  reason?: string;
  request?: string;
  task_status?: TaskStatus;
  chat_items?: number;
  has_active_task?: boolean;
  error_name?: string;
  error_message?: string;
};

