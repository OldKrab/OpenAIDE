import type { MessagePage } from "./chat.js";

export type RuntimeDiagnostics = {
  status: "ready" | "degraded";
  version?: string;
  method_count: number;
  tasks: {
    visible_count: number;
    total_count: number;
    active_count: number;
    active_tasks: Array<{
      task_id: string;
      agent_id: string;
      status: "preparing" | "starting" | "idle" | "running" | "stopping" | "waiting" | "interrupted" | "failed" | "completed";
      updated_at: string;
      last_activity: string;
      active_turn_id?: string;
      has_agent_session: boolean;
    }>;
    revision: number;
  };
  redaction: "prompt_text_file_contents_terminal_output_and_secrets_removed";
};

export type HealthResult = {
  status: string;
  version: string;
  methods: string[];
};

export type RuntimeSettingsResult = {
  developer: {
    acp_trace: {
      enabled: boolean;
      directory: string;
    };
  };
};
