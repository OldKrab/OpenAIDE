import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ActivityStep, ChatMessage } from "@openaide/app-shell-contracts";

describe("new task surface", () => {
  it("labels Agent option preparation while send is blocked", async () => {
    vi.stubGlobal("window", { acquireVsCodeApi: undefined });
    const { newTaskStatusLabel } = await import("./App");

    expect(
      newTaskStatusLabel({
        agentLabel: "OpenCode",
        configOptionsLoading: false,
        configOptionsReady: false,
        needsWorkspace: false,
        submitting: false,
      }),
    ).toBe("Preparing OpenCode options");
    expect(
      newTaskStatusLabel({
        agentLabel: "OpenCode",
        configOptionsReady: true,
        needsWorkspace: false,
        submitting: false,
      }),
    ).toBeUndefined();
  });
});

describe("tool detail rendering helpers", () => {
  beforeEach(() => {
    vi.stubGlobal("window", { acquireVsCodeApi: undefined });
  });

  it("accepts tool details without optional location or content arrays", async () => {
    const { firstToolPath } = await import("./App");
    const details = {
      input: {
        command: ["zsh", "-lc", "find . -iname '*readme*' -print"],
        cwd: "sample-workspace",
        fields: [],
      },
      output: {
        exit_code: 0,
        fields: [{ name: "status", value: "completed" }],
      },
    } as unknown as NonNullable<Extract<ActivityStep, { kind: "tool" }>["details"]>;

    expect(firstToolPath(details)).toBeUndefined();
  });

  it("falls back to input path when no explicit location exists", async () => {
    const { firstToolPath } = await import("./App");
    const details = {
      input: {
        command: [],
        path: "/workspace/README.md",
        fields: [],
      },
      output: {
        fields: [],
      },
    } as unknown as NonNullable<Extract<ActivityStep, { kind: "tool" }>["details"]>;

    expect(firstToolPath(details)).toEqual({ path: "/workspace/README.md", line: undefined });
  });
});

describe("task working status label", () => {
  beforeEach(() => {
    vi.stubGlobal("window", { acquireVsCodeApi: undefined });
  });

  it("describes the latest concrete activity with state-aware wording", async () => {
    const { taskWorkingStatusLabel } = await import("./App");

    expect(taskWorkingStatusLabel([], "active", false)).toBe("Starting");
    expect(taskWorkingStatusLabel([runningToolActivity("m1", "npm test")], "active", false)).toBe("Running npm test");
    expect(taskWorkingStatusLabel([
      runningToolActivity("m2", "rg streaming packages/frontend", "npm test --workspace openaide-frontend"),
    ], "active", false)).toBe("Running npm test --workspace openaide-frontend");
    expect(taskWorkingStatusLabel([namedToolActivity("m3", "read", "README.md", "running")], "active", false)).toBe(
      "Reading README.md",
    );
    expect(taskWorkingStatusLabel([namedToolActivity("m4", "read", "README.md", "completed")], "active", false)).toBe(
      "Read README.md",
    );
    expect(taskWorkingStatusLabel([completedCommandAndThoughtActivity("m5")], "active", false)).toBe("Thought");
  });

  it("advances from a tool group to its latest tool and then to the agent response", async () => {
    const { taskWorkingStatusLabel } = await import("./App");
    const activity = completedToolGroupActivity("m6");

    expect(taskWorkingStatusLabel([activity], "active", false)).toBe("Read README.md");
    expect(taskWorkingStatusLabel([activity, agentMessage("m7", "I found the cause.", true)], "active", false)).toBe(
      "Writing response",
    );
    expect(taskWorkingStatusLabel([activity, agentMessage("m7", "I found the cause.")], "active", false)).toBe(
      "Generated response",
    );
  });

  it("starts a new turn instead of reusing work from before the latest user message", async () => {
    const { taskWorkingStatusLabel } = await import("./App");
    const previousResponse = agentMessage("m8", "The first turn is complete.");
    const nextPrompt = userMessage("m9", "Now generate a prototype.");

    expect(taskWorkingStatusLabel([previousResponse, nextPrompt], "active", false)).toBe("Starting");
    expect(
      taskWorkingStatusLabel(
        [previousResponse, nextPrompt, namedToolActivity("m10", "read", "DESIGN.md", "running")],
        "active",
        false,
      ),
    ).toBe("Reading DESIGN.md");
  });

  it("does not expose ACP collaboration metadata while waiting for a subagent", async () => {
    const { taskWorkingStatusLabel } = await import("./App");
    const label = taskWorkingStatusLabel([collaborationWaitActivity("m6")], "active", false);

    expect(label).toBe("Waiting for subagent");
    expect(label).not.toContain("senderThreadId");
    expect(label).not.toContain("inProgress");
  });

  it("uses explicit labels for sending and blocked states", async () => {
    const { taskWorkingStatusLabel } = await import("./App");

    expect(taskWorkingStatusLabel([], "inactive", true)).toBe("Sending message");
    expect(taskWorkingStatusLabel([], "blocked", false)).toBe("Permission needed");
    expect(taskWorkingStatusLabel([systemMessage("app-server-preparation")], "blocked", false)).toBe("Preparing task");
    expect(taskWorkingStatusLabel([], "inactive", false)).toBeUndefined();
  });
});

describe("relativeTime", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-05-23T12:00:00.000Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("formats runtime epoch-millisecond timestamps", async () => {
    const { relativeTime } = await import("./App");

    expect(relativeTime(String(Date.parse("2026-05-23T11:57:00.000Z")))).toBe("3m");
  });

  it("formats native session ISO timestamps", async () => {
    const { relativeTime } = await import("./App");

    expect(relativeTime("2026-05-23T10:00:00.000Z")).toBe("2h");
  });
});

function runningToolActivity(id: string, inputSummary: string, ...laterInputSummaries: string[]): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "activity",
    message_id: id,
    message: {
      kind: "activity",
      id,
      title: "exec_command",
      status: "running",
      created_at: "2026-05-17T00:00:00Z",
      collapsed: true,
      steps: [inputSummary, ...laterInputSummaries].map((summary) => ({
        kind: "tool" as const,
        name: "execute",
        status: "running" as const,
        input_summary: summary,
      })),
    },
  };
}

function namedToolActivity(
  id: string,
  name: string,
  inputSummary: string,
  status: "running" | "completed",
): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "activity",
    message_id: id,
    message: {
      kind: "activity",
      id,
      title: name,
      status: "running",
      created_at: "2026-05-17T00:00:00Z",
      collapsed: true,
      steps: [{ kind: "tool", name, status, input_summary: inputSummary }],
    },
  };
}

function completedCommandAndThoughtActivity(id: string): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "activity",
    message_id: id,
    message: {
      kind: "activity",
      id,
      title: "Commands",
      status: "completed",
      created_at: "2026-05-17T00:00:00Z",
      collapsed: true,
      steps: [
        { kind: "tool", name: "execute", status: "completed", input_summary: "npm test" },
        { kind: "thought", text: "Review the result." },
      ],
    },
  };
}

function completedToolGroupActivity(id: string): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "activity",
    message_id: id,
    message: {
      kind: "activity",
      id,
      title: "Investigated the live activity status",
      status: "completed",
      created_at: "2026-05-17T00:00:00Z",
      collapsed: true,
      steps: [
        { kind: "tool", name: "search", status: "completed", input_summary: "live activity" },
        { kind: "tool", name: "read", status: "completed", input_summary: "README.md" },
      ],
    },
  };
}

function agentMessage(id: string, text: string, streaming = false): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "agent_text",
    message_id: id,
    message: {
      kind: "agent_text",
      id,
      text,
      streaming,
      created_at: "2026-05-17T00:00:01Z",
    },
  };
}

function userMessage(id: string, text: string): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "user",
    message_id: id,
    message: {
      kind: "user",
      id,
      text,
      created_at: "2026-05-17T00:00:02Z",
    },
  };
}

function collaborationWaitActivity(id: string): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "activity",
    message_id: id,
    message: {
      kind: "activity",
      id,
      title: "wait",
      status: "running",
      created_at: "2026-05-17T00:00:00Z",
      collapsed: true,
      steps: [{
        kind: "tool",
        name: "other",
        status: "running",
        input_summary: "senderThreadId 019f4bed-64e2-77f3-922a-d787c1d99968, status inProgress",
      }],
    },
  };
}

function systemMessage(id: string): ChatMessage {
  return {
    cursor: id,
    identity: id,
    message_type: "interruption",
    message_id: id,
    message: {
      kind: "interruption",
      id,
      reason: "backend_unavailable",
      message: "Preparing",
      created_at: "2026-05-17T00:00:00Z",
      recoverable: false,
    },
  };
}
