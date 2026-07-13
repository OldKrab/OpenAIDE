import { renderToStaticMarkup } from "react-dom/server";
import { isValidElement, type ReactElement, type ReactNode } from "react";
import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ActivityToolDetails, AgentMessagePart, Attachment, ChatMessage, PermissionOption } from "@openaide/app-shell-contracts";

describe("ChatRow", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    vi.resetModules();
    vi.stubGlobal("window", { acquireVsCodeApi: undefined });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("renders a copy action under user and agent messages", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const userHtml = renderToStaticMarkup(
      <ChatRow message={userMessage("u1", "copy me")} onPermissionRespond={vi.fn()} taskId="task_1" />,
    );
    const agentHtml = renderToStaticMarkup(
      <ChatRow message={agentMessage("a1", "copy agent")} onPermissionRespond={vi.fn()} taskId="task_1" />,
    );

    expect(userHtml).toContain('class="chat-message-actions end"');
    expect(userHtml).toContain('aria-label="Copy message"');
    expect(agentHtml).toContain('class="chat-agent-block"');
    expect(agentHtml).toContain('class="chat-message-actions start"');
    expect(agentHtml).toContain('aria-label="Copy message"');
  });

  it("shows a caret only when live Frontend presentation requests it", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const streamingHtml = renderToStaticMarkup(
      <ChatRow
        message={agentMessage("a1", "The complete received chunk is visible now.")}
        onPermissionRespond={vi.fn()}
        showStreamingCaret
        taskId="task_1"
      />,
    );
    const completedHtml = renderToStaticMarkup(
      <ChatRow
        message={agentMessage("a1", "The complete received chunk is visible now.")}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(streamingHtml).toContain("The complete received chunk is visible now.");
    expect(streamingHtml).toContain('class="chat-streaming-caret"');
    expect(streamingHtml).toContain('aria-busy="true"');
    expect(completedHtml).not.toContain("chat-streaming-caret");
    expect(completedHtml).not.toContain('aria-busy="true"');
  });

  it("renders appended live Agent text on the update that receives it", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const props = { onPermissionRespond: vi.fn(), taskId: "task_1" };
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatRow {...props} message={agentMessage("a1", "Agent")} showStreamingCaret />);
    });
    act(() => {
      tree!.update(
        <ChatRow
          {...props}
          message={agentMessage("a1", "Agent text arrives as one larger network chunk.")}
          showStreamingCaret
        />,
      );
    });

    const rendered = JSON.stringify(tree!.toJSON());
    expect(rendered).toContain("Agent text arrives as one larger network chunk.");
    expect(rendered).toContain("Copy message");
    expect(rendered).toContain("chat-streaming-caret");
  });

  it("renders thought messages as their own collapsed row", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow message={thoughtMessage("t1", "Check current files first.")} onPermissionRespond={vi.fn()} taskId="task_1" />,
    );

    expect(html).toContain("chat-thought-block");
    expect(html).toContain("<span>Thinking</span>");
    expect(html).toContain("Check current files first.");
  });

  it("keeps a running activity group collapsed even when its source requests expansion", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={{
          cursor: "cursor_activity",
          identity: "activity_running",
          message_type: "activity",
          message_id: "activity_running",
          message: {
            kind: "activity",
            id: "activity_running",
            title: "Commands",
            status: "running",
            created_at: "2026-05-23T00:00:00Z",
            collapsed: false,
            steps: [{ kind: "tool", name: "execute", status: "running", input_summary: "npm test" }],
          },
        }}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain('class="activity-group running"');
    expect(html).toContain('class="activity-group running"><button aria-expanded="false"');
  });

  it("renders recoverable interruptions as compact recovery status rows", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={interruptionMessage("i1", "Task was stopped because OpenAIDE restarted.", true)}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain('class="recovery-banner"');
    expect(html).toContain('role="status"');
    expect(html).toContain("Task was stopped because OpenAIDE restarted.");
    expect(html).not.toContain('class="chat-system"');
  });

  it("keeps non-recoverable interruptions as plain system text", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow message={interruptionMessage("i1", "Task failed.", false)} onPermissionRespond={vi.fn()} taskId="task_1" />,
    );

    expect(html).toContain('class="chat-system"');
    expect(html).not.toContain('class="recovery-banner"');
  });

  it("renders thought steps inside activity groups as collapsed thought rows", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={{
          ...toolActivity("read", "Read notes.md", emptyToolDetails()),
          message: {
            kind: "activity",
            id: "activity_mixed",
            title: "Tool activity",
            status: "completed",
            created_at: "2026-05-23T00:00:00Z",
            collapsed: false,
            steps: [
              { kind: "thought", text: "Check current files first." },
              { kind: "tool", name: "read", status: "completed", input_summary: "notes.md" },
            ],
          },
        }}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain("Thought, read file");
    expect(html).toContain("activity-thought-block");
    expect(html).toContain('<span class="activity-step-title">Thought</span>');
    expect(html).toContain("Check current files first.");
  });

  it("does not invent details for summary-only tool rows", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={{
          cursor: "cursor_activity",
          identity: "activity_summary_only",
          message_type: "activity",
          message_id: "activity_summary_only",
          message: {
            kind: "activity",
            id: "activity_summary_only",
            title: "Tool activity",
            status: "completed",
            created_at: "2026-05-23T00:00:00Z",
            collapsed: false,
            steps: [{ kind: "tool", name: "other", status: "completed", input_summary: "Read file '/workspace/a.ts'" }],
          },
        }}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain("Read file");
    expect(html).not.toContain("activity-tool-details");
  });

  it("does not invent details for legacy tool-like text rows", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={{
          cursor: "cursor_activity",
          identity: "activity_text_tools",
          message_type: "activity",
          message_id: "activity_text_tools",
          message: {
            kind: "activity",
            id: "activity_text_tools",
            title: "Tool activity",
            status: "completed",
            created_at: "2026-05-23T00:00:00Z",
            collapsed: false,
            steps: [{ kind: "text", text: "Read file '/workspace/a.ts'" }],
          },
        }}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain("Read file");
    expect(html).not.toContain("activity-tool-details");

    const editHtml = renderToStaticMarkup(
      <ChatRow
        message={{
          cursor: "cursor_activity",
          identity: "activity_text_edit",
          message_type: "activity",
          message_id: "activity_text_edit",
          message: {
            kind: "activity",
            id: "activity_text_edit",
            title: "Editing files",
            status: "completed",
            created_at: "2026-05-23T00:00:00Z",
            collapsed: false,
            steps: [{ kind: "text", text: "Editing files" }],
          },
        }}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(editHtml).toContain("Updated file");
    expect(editHtml).toContain("Editing files");
    expect(editHtml).not.toContain("activity-tool-details");
  });

  it("renders command rows as expandable activity details", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={{
          cursor: "cursor_activity",
          identity: "activity_command",
          message_type: "activity",
          message_id: "activity_command",
          message: {
            kind: "activity",
            id: "activity_command",
            title: "Commands",
            status: "completed",
            created_at: "2026-05-23T00:00:00Z",
            collapsed: false,
            steps: [{ kind: "command", command_label: "npm test", status: "completed", exit_code: 0, output_preview: "passed" }],
          },
        }}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain("Ran command");
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("activity-disclosure-body");
    expect(html).toContain('aria-hidden="true"');
    expect(html).toContain('inert=""');
    expect(html).toContain(
      '<span class="activity-step-action">Ran</span><code class="activity-step-command">npm test</code>',
    );
    expect(html).toContain("passed");
  });

  it("renders legacy command-shaped rows as expandable activity details", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={{
          cursor: "cursor_activity",
          identity: "activity_legacy_command",
          message_type: "activity",
          message_id: "activity_legacy_command",
          message: {
            kind: "activity",
            id: "activity_legacy_command",
            title: "Commands",
            status: "completed",
            created_at: "2026-05-23T00:00:00Z",
            collapsed: false,
            steps: [{ kind: "text", text: "git status --short openaide-rs/app-server-protocol/src/snapshot/chat.rs" }],
          },
        }}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain("Ran command");
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("activity-disclosure-body");
    expect(html).toContain("activity-tool-details");
    expect(html).toContain("git status --short openaide-rs/app-server-protocol/src/snapshot/chat.rs");

    const toolHtml = renderToStaticMarkup(
      <ChatRow
        message={{
          cursor: "cursor_activity",
          identity: "activity_legacy_command_tool",
          message_type: "activity",
          message_id: "activity_legacy_command_tool",
          message: {
            kind: "activity",
            id: "activity_legacy_command_tool",
            title: "Tool activity",
            status: "completed",
            created_at: "2026-05-23T00:00:00Z",
            collapsed: false,
            steps: [{ kind: "tool", name: "other", status: "completed", input_summary: "git status --short" }],
          },
        }}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(toolHtml).toContain("Ran command");
    expect(toolHtml).toContain('aria-expanded="false"');
    expect(toolHtml).toContain("activity-disclosure-body");
    expect(toolHtml).toContain("activity-tool-details");
    expect(toolHtml).toContain("git status --short");
  });

  it("renders a single image attachment as prominent media before user text", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={userMessage("u1", "Inspect this", [
          { kind: "file", label: "diagram.png", payload: { data: "aW1hZ2U=", mimeType: "image/png" } },
          { kind: "file", label: "notes.md" },
        ])}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain('aria-label="Open diagram.png"');
    expect(html).toContain('class="chat-image-grid" data-layout="single"');
    expect(html).toContain('class="chat-image-preview"');
    expect(html).not.toContain('class="context-token-label">diagram.png</span>');
    expect(html).toContain('src="data:image/png;base64,aW1hZ2U="');
    expect(html).toContain('class="chat-attachment-chip"><svg');
    expect(html).toContain("notes.md");
    expect(html.indexOf("chat-image-grid")).toBeLessThan(html.indexOf("Inspect this"));
  });

  it("renders typed Agent images, resources, and unsupported content explicitly", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const imageHtml = renderToStaticMarkup(
      <ChatRow
        message={agentContentMessage("image", {
          kind: "image",
          media_type: "image/png",
          data_url: "data:image/png;base64,aW1hZ2U=",
          uri: "memory://diagram.png",
        })}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );
    const resourceHtml = renderToStaticMarkup(
      <ChatRow
        message={agentContentMessage("resource", {
          kind: "resource",
          uri: "memory://notes.txt",
          media_type: "text/plain",
          text: "Embedded notes",
        })}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );
    const unsupportedHtml = renderToStaticMarkup(
      <ChatRow
        message={agentContentMessage("audio", {
          kind: "unsupported",
          content_type: "audio",
          media_type: "audio/wav",
        })}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(imageHtml).toContain('aria-label="Open diagram.png"');
    expect(imageHtml).toContain('src="data:image/png;base64,aW1hZ2U="');
    expect(resourceHtml).toContain("notes.txt");
    expect(resourceHtml).toContain("Embedded notes");
    expect(unsupportedHtml).toContain("Audio output is not previewable yet.");
    expect(unsupportedHtml).toContain("audio/wav");
  });

  it("renders every mixed Agent message part in its original order", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={{
          cursor: "agent-mixed",
          identity: "agent-mixed",
          message_id: "agent-mixed",
          message_type: "agent_message",
          message: {
            kind: "agent_message",
            id: "agent-mixed",
            role: "agent",
            parts: [
              { kind: "text", text: "Before content" },
              { kind: "resource", uri: "memory://result.txt", text: "Resource content" },
              { kind: "text", text: "After content" },
              { kind: "unsupported", content_type: "audio", media_type: "audio/wav" },
            ],
            created_at: "2026-05-23T00:00:00Z",
          },
        }}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    const orderedText = ["Before content", "result.txt", "After content", "Audio output"];
    const positions = orderedText.map((text) => html.indexOf(text));
    expect(positions.every((position) => position >= 0)).toBe(true);
    expect(positions).toEqual([...positions].sort((left, right) => left - right));
  });

  it("renders an attachment-only user message without empty text or copy controls", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={userMessage("u1", "", [
          { kind: "file", label: "diagram.png", payload: { data: "aW1hZ2U=", mimeType: "image/png" } },
        ])}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain('aria-label="Open diagram.png"');
    expect(html).not.toContain('class="chat-user"');
    expect(html).not.toContain('aria-label="Copy message"');
  });

  it("renders image markdown embedded in user message text as an openable preview without a visible filename", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const payload = "aW1hZ2U=".repeat(600);
    const html = renderToStaticMarkup(
      <ChatRow
        message={userMessage("u1", `what did you see here\n\n[@image](data:image/png;base64,${payload})`)}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain('aria-label="Open @image"');
    expect(html).toContain('class="chat-user-image-link"');
    expect(html).toContain(`src="data:image/png;base64,${payload}"`);
    expect(html).not.toContain("[@image]");
    expect(html).not.toContain("<span>@image</span>");
  });

  it("derives chat image preview sources from safe attachment payloads", async () => {
    const { chatImagePreview } = await import("./AttachmentImagePreview");

    expect(chatImagePreview({ kind: "file", label: "diagram.png", payload: { data: "aW1hZ2U=", mimeType: "image/png" } })).toEqual({
      label: "diagram.png",
      url: "data:image/png;base64,aW1hZ2U=",
    });
    expect(chatImagePreview({ kind: "file", label: "diagram.png", payload: { previewUrl: "data:image/png;base64,aW1hZ2U=" } })).toEqual({
      label: "diagram.png",
      url: "data:image/png;base64,aW1hZ2U=",
    });
    expect(chatImagePreview({ kind: "file", label: "notes.md", payload: { data: "bm90ZXM=", mimeType: "text/plain" } })).toBeUndefined();
  });

  it("renders every same-named image attachment in its original order", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={userMessage("u1", "Compare these", [
          { kind: "file", label: "image.png", payload: { previewUrl: "data:image/png;base64,Zmlyc3Q=" } },
          { kind: "file", label: "image.png", payload: { previewUrl: "data:image/png;base64,c2Vjb25k" } },
          { kind: "file", label: "image.png", payload: { previewUrl: "data:image/png;base64,dGhpcmQ=" } },
        ])}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html.match(/aria-label="Open image\.png"/g)).toHaveLength(3);
    expect(html).toContain('class="chat-image-grid" data-layout="many"');
    expect(html.indexOf("Zmlyc3Q=")).toBeLessThan(html.indexOf("c2Vjb25k"));
    expect(html.indexOf("c2Vjb25k")).toBeLessThan(html.indexOf("dGhpcmQ="));
  });

  it("renders typed read, edit, search, and execute tool details", async () => {
    const { ChatRow } = await import("./ChatMessageView");

    const readHtml = renderToStaticMarkup(
      <ChatRow
        message={toolActivity("read", "Read notes.md", {
          locations: [{ path: "/workspace/notes.md" }],
          content: [{ kind: "text", text: "alpha\nbeta" }],
          input: input({ command: ["tail", "-n", "2", "notes.md"] }),
          output: undefined,
        })}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );
    const editHtml = renderToStaticMarkup(
      <ChatRow
        message={toolActivity("edit", "Create notes.md", {
          locations: [],
          content: [{ kind: "diff", path: "/workspace/notes.md", new_text: "# Project Notes\n\n- [x] Define scope" }],
          input: input(),
          output: { success: true, fields: [] },
        })}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );
    const searchHtml = renderToStaticMarkup(
      <ChatRow
        message={toolActivity("search", 'Find "beta"', {
          locations: [],
          content: [],
          input: input({ command: ["rg", "-n", "beta", "."], query: "beta", cwd: "/workspace" }),
          output: { stdout: "notes.md:2:beta: second checkpoint", fields: [] },
        })}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );
    const executeHtml = renderToStaticMarkup(
      <ChatRow
        message={toolActivity("execute", "pwd", {
          locations: [],
          content: [],
          input: input({ command: ["/usr/bin/zsh", "-lc", "pwd"] }),
          output: { stdout: "/workspace", exit_code: 0, success: true, fields: [] },
        })}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(readHtml).toContain("activity-tool-read-detail");
    expect(readHtml).toContain("read-tool-line-number");
    expect(readHtml).toContain("read-tool-command");
    expect(editHtml).toContain("activity-tool-edit-detail");
    expect(editHtml).toContain("@@ -0,0 +1,3 @@");
    expect(editHtml).toContain("edit-tool-old-line-number");
    expect(editHtml).toContain("edit-tool-new-line-number");
    expect(editHtml).not.toContain("@@ (new file) @@");
    expect(editHtml).toContain("Created workspace/notes.md");
    expect(searchHtml).toContain("activity-tool-search-detail matched");
    expect(searchHtml).toContain("<mark>beta</mark>");
    expect(executeHtml).toContain("activity-tool-execute-detail completed");
    expect(executeHtml).toContain(
      '<span class="activity-step-action">Ran</span><code class="activity-step-command">pwd</code>',
    );
    expect(executeHtml).toContain("exit 0");
    expect(executeHtml).not.toContain("/usr/bin/zsh -lc");
  });

  it("renders unloaded execute tool artifacts as command activity", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={{
          cursor: "cursor_execute_artifact",
          identity: "activity_execute_artifact",
          message_type: "activity",
          message_id: "activity_execute_artifact",
          message: {
            kind: "activity",
            id: "activity_execute_artifact",
            title: "printf 'openaide-permission-test-allow-once\\n'",
            status: "completed",
            created_at: "2026-05-23T00:00:00Z",
            collapsed: false,
            steps: [
              {
                kind: "tool",
                name: "execute",
                status: "completed",
                input_summary: "zsh -lc \"printf 'openaide-permission-test-allow-once\\\\n'\"",
                output_preview: "Terminal output",
                detail_artifact_id: "artifact_1",
              },
            ],
          },
        }}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain("Ran command");
    expect(html).not.toContain("Called tool");
    expect(html).toContain('class="activity-step tool-execute completed"');
    expect(html).toContain("lucide-terminal activity-kind-icon");
    expect(html).toContain("activity-tool-execute-detail completed");
    expect(html).toContain("execute-command-chip");
    expect(html).toContain("printf &#x27;openaide-permission-test-allow-once");
    expect(html).not.toContain("<code>Terminal output</code>");
  });

  it("renders execute permission requests with approval controls", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={permissionMessage("p1", "mkdir .openaide-acp-tool-fixture/archive", [
          { id: "approved", label: "Yes, proceed", kind: "allow" },
          { id: "approved-execpolicy-amendment", label: "Yes, and don't ask again for commands that start with `mkdir`", kind: "allow" },
          { id: "abort", label: "No, and tell Codex what to do differently", kind: "deny" },
        ])}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain("Approval required");
    expect(html).toContain("<strong>Approve command</strong>");
    expect(html).toContain("execute-command-chip");
    expect(html).toContain("permission-body");
    expect(html).toContain('class="remember"');
  });

  it("renders resolved denied permissions as answered history blocks even without a selected option", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const message = permissionMessage("p1", "npm exec --workspace openaide-frontend", [
      { id: "allow_once", label: "Allow Once", kind: "allow" },
      { id: "reject_once", label: "Reject", kind: "deny" },
    ]);
    message.message = {
      ...message.message,
      state: "resolved",
      decision: "denied",
    } as Extract<ChatMessage["message"], { kind: "permission" }>;

    const html = renderToStaticMarkup(
      <ChatRow
        message={message}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain("permission-card");
    expect(html).toContain("Denied");
    expect(html).not.toContain("permission-resolution");
    expect(html).not.toContain("Answer:");
    expect(html).not.toContain("Cancelled");
    expect(html).not.toContain("Approval required");
    expect(html).not.toContain("Allow Once");
  });

  it("renders cancelled permissions without calling them denied", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const message = permissionMessage("p1", "npm exec --workspace openaide-frontend", [
      { id: "allow_once", label: "Allow Once", kind: "allow" },
      { id: "reject_once", label: "Reject", kind: "deny" },
    ]);
    message.message = {
      ...message.message,
      state: "cancelled",
    } as Extract<ChatMessage["message"], { kind: "permission" }>;

    const html = renderToStaticMarkup(
      <ChatRow
        message={message}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain("permission-card");
    expect(html).toContain("Permission request cancelled");
    expect(html).not.toContain("Denied");
    expect(html).not.toContain("Approval required");
    expect(html).not.toContain("Allow Once");
  });

  it("uses explicit permission lifecycle labels", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const base = permissionMessage("p1", "npm run web:target:restart", [
      { id: "allow_once", label: "Allow Once", kind: "allow" },
      { id: "allow_session", label: "Allow for Session", kind: "allow" },
      { id: "reject_once", label: "Reject", kind: "deny" },
    ]);
    const renderPermission = (
      overrides: Partial<Extract<ChatMessage["message"], { kind: "permission" }>>,
      response?: { responding: boolean; error?: string },
    ) =>
      renderToStaticMarkup(
        <ChatRow
          message={{ ...base, message: { ...base.message, ...overrides } as Extract<ChatMessage["message"], { kind: "permission" }> }}
          onPermissionRespond={vi.fn()}
          permissionResponse={response}
          taskId="task_1"
        />,
      );

    expect(renderPermission({})).toContain("Approval required");
    expect(renderPermission({}, { responding: true })).toContain("Sending response");
    expect(renderPermission({ state: "resolved", decision: "approved", selected_option: "allow_session" })).toContain(
      "Approved, Allow for Session",
    );
    expect(renderPermission({ state: "resolved", decision: "denied", selected_option: "reject_once" })).toContain(
      "Denied, Reject",
    );
    expect(renderPermission({ state: "cancelled" })).toContain("Permission request cancelled");
    expect(renderPermission({
      state: "cancelled",
      resolution_message: "Task stopped while approval was pending.",
    })).toContain("Task stopped while approval was pending.");
  });

  it("uses command option text instead of the generic Tool call permission placeholder", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={permissionMessage("p1", "Tool call", [
          { id: "allow_once", label: "Allow Once", kind: "allow" },
          { id: "allow_always", label: "Allow for Session", kind: "allow" },
          {
            id: "accept_execpolicy_amendment",
            label: "Allow Commands Starting With `node /tmp/openaide-pw/verify-tool-activity.mjs`",
            kind: "allow",
          },
          { id: "reject_once", label: "Reject", kind: "deny" },
        ])}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain("node /tmp/openaide-pw/verify-tool-activity.mjs");
    expect(html).not.toContain("<strong>Tool call</strong>");
    expect(html).not.toContain("&gt;_ Tool call");
  });

  it("explains OpenCode external directory permission requests", async () => {
    const { ChatRow } = await import("./ChatMessageView");
    const html = renderToStaticMarkup(
      <ChatRow
        message={permissionMessage("p1", "external_directory", [
          { id: "allow_once", label: "Allow once", kind: "allow" },
          { id: "reject", label: "Reject", kind: "deny" },
        ], "other")}
        onPermissionRespond={vi.fn()}
        taskId="task_1"
      />,
    );

    expect(html).toContain("External directory access");
    expect(html).toContain("outside the current workspace");
    expect(html).not.toContain("&gt;_ external_directory");
  });

  it("subscribes to tool details only while the rendered disclosure is open", async () => {
    vi.useFakeTimers();
    const { ActivityStepRow } = await import("./ChatActivityView");

    const cleanup = vi.fn();
    const onSubscribeToolDetail = vi.fn(() => cleanup);
    const step = {
      kind: "tool" as const,
      name: "read",
      status: "running" as const,
      input_summary: "Read notes.md",
      detail_artifact_id: "artifact_1",
    };
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<ActivityStepRow onSubscribeToolDetail={onSubscribeToolDetail} step={step} taskId="task_1" toolDetails={{}} />);
    });
    act(() => tree.root.findByProps({ className: "activity-disclosure-trigger" }).props.onClick());
    expect(onSubscribeToolDetail).toHaveBeenCalledOnce();
    expect(onSubscribeToolDetail).toHaveBeenCalledWith("artifact_1");
    act(() => {
      vi.advanceTimersByTime(250);
    });
    expect(onSubscribeToolDetail).toHaveBeenCalledOnce();

    act(() => tree.root.findByProps({ className: "activity-disclosure-trigger" }).props.onClick());
    expect(cleanup).toHaveBeenCalledOnce();
    act(() => tree.unmount());

    const loadedHtml = renderToStaticMarkup(
      ActivityStepRow({
        onSubscribeToolDetail,
        step: { ...step, name: "edit", input_summary: undefined },
        taskId: "task_1",
        toolDetails: {
          ["task_1\u0000artifact_1"]: {
            loading: false,
            details: {
              locations: [],
              content: [{ kind: "diff", path: "/workspace/src/notes.md", new_text: "updated" }],
              input: input(),
              output: { success: true, fields: [] },
            },
          },
        },
      }),
    );
    expect(loadedHtml).toContain("Edit notes.md");
  });

  it("keeps search scope visible beside a failed status", async () => {
    const { ActivityStepRow } = await import("./ChatActivityView");
    const html = renderToStaticMarkup(
      ActivityStepRow({
        step: {
          kind: "tool",
          name: "search",
          status: "error",
          input_summary: "Search for 'activity' in frontend",
        },
        taskId: "task_1",
      }),
    );

    expect(html).toContain('class="activity-step-context">frontend</small>');
    expect(html).toContain('class="activity-step-state">Failed</small>');
  });

  it("renders web search as its own compact tool row", async () => {
    const { ActivityStepRow } = await import("./ChatActivityView");
    const html = renderToStaticMarkup(
      ActivityStepRow({
        step: {
          kind: "tool",
          name: "web_search",
          status: "completed",
          input_summary: "Saint Petersburg weather tomorrow",
          details: {
            locations: [],
            content: [],
            input: {
              command: [],
              query: "Saint Petersburg weather tomorrow",
              queries: [
                "Saint Petersburg weather tomorrow",
                "Санкт-Петербург погода завтра",
              ],
              fields: [{ name: "type", value: { kind: "string", value: "webSearch" } }],
            },
          },
        },
        taskId: "task_1",
      }),
    );

    expect(html).toContain("lucide-earth activity-kind-icon");
    expect(html).toContain("Web search: Saint Petersburg weather tomorrow");
    expect(html).toContain("activity-tool-web-search-detail");
    expect(html).toContain('class="web-search-tool-queries"');
    expect(html).toContain("Saint Petersburg weather tomorrow</li>");
    expect(html).toContain("Санкт-Петербург погода завтра</li>");
    expect(html).not.toContain("No matches in .");
  });

  it("reclassifies persisted generic searches when their web-search details load", async () => {
    const { ActivityStepRow } = await import("./ChatActivityView");
    const html = renderToStaticMarkup(
      ActivityStepRow({
        step: {
          kind: "tool",
          name: "search",
          status: "completed",
          input_summary: "id exec-internal, type webSearch",
          details: {
            locations: [],
            content: [],
            input: {
              command: [],
              fields: [{ name: "type", value: { kind: "string", value: "webSearch" } }],
            },
          },
        },
        taskId: "task_1",
      }),
    );

    expect(html).toContain('class="activity-step tool-web_search completed"');
    expect(html).toContain("lucide-earth activity-kind-icon");
    expect(html).toContain('<span class="activity-step-title">Web search</span>');
    expect(html).not.toContain("exec-internal");
    expect(html).not.toContain("No matches in .");
  });

  it("aligns thinking and summary-only tools with expandable activity rows", async () => {
    const { ActivityStepRow } = await import("./ChatActivityView");
    const thinkingHtml = renderToStaticMarkup(
      ActivityStepRow({
        step: { kind: "thought", text: "Inspect the activity presentation." },
        taskId: "task_1",
      }),
    );
    const skillHtml = renderToStaticMarkup(
      ActivityStepRow({
        step: { kind: "tool", name: "skill", status: "completed", input_summary: "Activated impeccable" },
        taskId: "task_1",
      }),
    );

    expect(thinkingHtml).toContain('class="activity-step activity-thought-block"');
    expect(thinkingHtml).toContain('class="activity-step-main"');
    expect(thinkingHtml).toContain("lucide-brain activity-kind-icon");
    expect(skillHtml).toContain('class="activity-step-disclosure-placeholder"');
    expect(skillHtml).toContain("lucide-book-open activity-kind-icon");
  });

  it("renders think tools as identified thought-like tool disclosures", async () => {
    const { ActivityStepRow } = await import("./ChatActivityView");
    const html = renderToStaticMarkup(
      ActivityStepRow({
        step: {
          kind: "tool",
          tool_call_id: "tool-think-1",
          name: "think",
          status: "completed",
          details: {
            locations: [],
            content: [{ kind: "text", text: "Compare the two architectures." }],
          },
        },
        taskId: "task_1",
      }),
    );

    expect(html).toContain('data-step-id="tool-think-1"');
    expect(html).toContain("lucide-brain-circuit activity-kind-icon");
    expect(html).toContain("Reasoning tool");
    expect(html).toContain("Compare the two architectures.");
    expect(html).not.toContain('class="activity-step activity-thought-block"');
  });

  it("renders typed image, audio, resource, and unsupported tool content", async () => {
    const { ChatToolDetails } = await import("./ChatToolDetailsView");
    const html = renderToStaticMarkup(
      <ChatToolDetails
        details={{
          locations: [],
          content: [
            { kind: "image", media_type: "image/png", data_url: "data:image/png;base64,aW1hZ2U=", uri: "file:///preview.png" },
            { kind: "audio", media_type: "audio/wav", data_url: "data:audio/wav;base64,YXVkaW8=" },
            { kind: "resource", uri: "https://example.test/guide", name: "Guide", description: "Reference guide", media_type: "text/markdown", size_bytes: 42 },
            { kind: "unsupported", content_type: "resource_blob", media_type: "application/octet-stream", uri: "file:///archive.bin" },
          ],
          input: {
            command: [],
            fields: [
              {
                name: "filters",
                value: {
                  kind: "object",
                  fields: [{ name: "languages", value: { kind: "array", items: [
                    { kind: "string", value: "rust" },
                    { kind: "string", value: "typescript" },
                  ] } }],
                },
              },
              { name: "api_token", value: { kind: "redacted" } },
            ],
          },
        }}
        step={{ kind: "tool", name: "fetch", status: "completed" }}
      />,
    );

    expect(html).toContain('<img alt="Tool output"');
    expect(html).toContain("file:///preview.png");
    expect(html).toContain('class="activity-tool-audio" controls=""');
    expect(html).toContain("Guide");
    expect(html).toContain("Reference guide");
    expect(html).toContain("text/markdown · 42 bytes");
    expect(html).toContain("Unsupported resource blob");
    expect(html).toContain("application/octet-stream");
    expect(html).toContain("languages");
    expect(html).toContain("rust, typescript");
    expect(html).toContain("[redacted]");
  });

  it("delays tool-detail loading UI and replaces it with content", async () => {
    vi.useFakeTimers();
    const { ChatToolDetails } = await import("./ChatToolDetailsView");
    const step = {
      kind: "tool" as const,
      name: "read",
      status: "running" as const,
      input_summary: "Read notes.md",
    };
    let tree: ReturnType<typeof create>;
    act(() => {
      tree = create(<ChatToolDetails loading step={step} />);
    });

    expect(JSON.stringify(tree!.toJSON())).not.toContain("Loading details");
    expect(JSON.stringify(tree!.toJSON())).not.toContain("activity-tool-skeleton");

    act(() => {
      vi.advanceTimersByTime(149);
    });
    expect(JSON.stringify(tree!.toJSON())).not.toContain("activity-tool-skeleton");

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(JSON.stringify(tree!.toJSON())).toContain("activity-tool-skeleton");

    act(() => {
      tree!.update(
        <ChatToolDetails
          details={{ locations: [], content: [{ kind: "text", text: "loaded details" }] }}
          step={{ ...step, status: "completed" }}
        />,
      );
    });
    const loaded = JSON.stringify(tree!.toJSON());
    expect(loaded).toContain("loaded details");
    expect(loaded).not.toContain("activity-tool-skeleton");
    vi.useRealTimers();
  });

  it("opens tool paths through the rendered tool path button", async () => {
    const posted: unknown[] = [];
    vi.stubGlobal("window", { acquireVsCodeApi: () => ({ postMessage: (message: unknown) => posted.push(message) }) });
    const [{ installFrontendShell }, { createVsCodeShell }, { ToolPath, toolOpenPathMessage }] = await Promise.all([
      import("../services/frontendShell"),
      import("../../../../apps/vscode-extension/frontend/vsCodeShell"),
      import("./ChatToolBlocks"),
    ]);
    installFrontendShell(createVsCodeShell());
    expect(toolOpenPathMessage({ path: "/workspace/notes.md" })).toEqual({
      type: "tool.openPath",
      payload: { line: undefined, path: "/workspace/notes.md" },
    });
    expect(toolOpenPathMessage({ line: 12, path: "/workspace/notes.md" })).toEqual({
      type: "tool.openPath",
      payload: { line: 12, path: "/workspace/notes.md" },
    });

    const button = ToolPath({ line: 12, path: "/workspace/notes.md" });
    button.props.onClick({ preventDefault: vi.fn(), stopPropagation: vi.fn() });
    expect(posted).toEqual([{ type: "tool.openPath", payload: { line: 12, path: "/workspace/notes.md" } }]);
  });

  it("normalizes file-list search paths before rendering openable tool paths", async () => {
    const { SearchToolDetails } = await import("./SearchToolDetails");
    const { ToolPath } = await import("./ChatToolBlocks");
    const element = SearchToolDetails({
      details: {
        locations: [],
        content: [],
        input: input({ command: ["rg", "--files"], cwd: "/workspace" }),
        output: { stdout: "src/a.ts", exit_code: 0, fields: [] },
      },
      step: {
        kind: "tool",
        name: "search",
        status: "completed",
        input_summary: "Find files",
      },
    });

    const path = findElement(element, (candidate) => candidate.type === ToolPath);
    expect(path.props.label).toBe("src/a.ts");
    expect(path.props.path).toBe("/workspace/src/a.ts");
  });

  it("maps rendered permission buttons to allow and deny decisions only", async () => {
    const { ChatPermissionCard, permissionDecisionForOption } = await import("./ChatPermissionCard");
    expect(permissionDecisionForOption({ id: "allow_once", label: "Allow once", kind: "allow" })).toBe("approved");
    expect(permissionDecisionForOption({ id: "reject", label: "Reject", kind: "deny" })).toBe("denied");
    expect(permissionDecisionForOption({ id: "remember", label: "Remember", kind: "other" })).toBeUndefined();

    const onRespond = vi.fn();
    const permission = permissionMessage("p1", "mkdir archive", [
      { id: "allow_once", label: "Allow once", kind: "allow" },
      { id: "remember", label: "Remember", kind: "other" },
      { id: "reject", label: "Reject", kind: "deny" },
    ]).message as Extract<ChatMessage["message"], { kind: "permission" }>;
    const element = ChatPermissionCard({ permission, onRespond });
    const buttons = findElements(element, (candidate) => candidate.type === "button");
    expect(buttons[1].props.disabled).toBe(true);
    buttons[0].props.onClick(permissionActionEvent());
    buttons[2].props.onClick(permissionActionEvent());
    expect(onRespond).toHaveBeenNthCalledWith(1, "request_p1", "allow_once");
    expect(onRespond).toHaveBeenNthCalledWith(2, "request_p1", "reject");

    const appServerPermission = {
      ...permission,
      app_server_request_id: "server-request-1",
    };
    findElements(ChatPermissionCard({ permission: appServerPermission, onRespond }), (candidate) => candidate.type === "button")[0]
      .props.onClick(permissionActionEvent());
    expect(onRespond).toHaveBeenLastCalledWith("server-request-1", "allow_once");

    const respondingElement = ChatPermissionCard({
      permission: permissionMessage("p1", "mkdir archive", [{ id: "allow_once", label: "Allow once", kind: "allow" }]).message as Extract<
        ChatMessage["message"],
        { kind: "permission" }
      >,
      response: { responding: true },
      onRespond,
    });
    expect(findElements(respondingElement, (candidate) => candidate.type === "button")[0].props.disabled).toBe(true);

    const failedElement = ChatPermissionCard({
      permission,
      response: { responding: false, error: "Permission response failed" },
      onRespond,
    });
    const error = findElement(failedElement, (candidate) => candidate.props.className === "permission-error");
    expect(error.props.role).toBe("alert");
  });

  it("moves focus off the action before resolving a permission", async () => {
    const { ChatPermissionCard } = await import("./ChatPermissionCard");
    const focus = vi.fn();
    const onRespond = vi.fn();
    const permission = permissionMessage("p1", "mkdir archive", [
      { id: "allow_once", label: "Allow once", kind: "allow" },
    ]).message as Extract<ChatMessage["message"], { kind: "permission" }>;
    const element = ChatPermissionCard({ permission, onRespond });
    const card = findElement(element, (candidate) => candidate.type === "section");
    const action = findElement(element, (candidate) => candidate.props.className === "allow");
    const status = findElement(element, (candidate) => (
      typeof candidate.props.className === "string" && candidate.props.className.startsWith("permission-state")
    ));
    action.props.onClick({
      currentTarget: {
        closest: () => ({ focus }),
      },
    });

    expect(card.props.tabIndex).toBe(-1);
    expect(status.props).toMatchObject({
      "aria-atomic": "true",
      "aria-live": "polite",
      role: "status",
    });
    expect(focus).toHaveBeenCalledWith({ preventScroll: true });
    expect(onRespond).toHaveBeenCalledWith("request_p1", "allow_once");
  });
});

function findElement(element: ReactNode, predicate: (element: ReactElement<Record<string, any>>) => boolean) {
  const matches = findElements(element, predicate);
  if (!matches[0]) throw new Error("Expected React element was not found.");
  return matches[0];
}

function permissionActionEvent() {
  return { currentTarget: { closest: () => null } };
}

function findElements(element: ReactNode, predicate: (element: ReactElement<Record<string, any>>) => boolean) {
  const matches: Array<ReactElement<Record<string, any>>> = [];
  const visit = (node: ReactNode) => {
    if (Array.isArray(node)) {
      node.forEach(visit);
      return;
    }
    if (!isValidElement<Record<string, any>>(node)) return;
    if (predicate(node)) matches.push(node);
    visit(node.props.children);
  };
  visit(element);
  return matches;
}

function userMessage(id: string, text: string, attachments?: Attachment[]): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "user",
    message_id: id,
    message: {
      kind: "user",
      id,
      text,
      created_at: "2026-05-23T00:00:00Z",
      attachments,
    },
  };
}

function agentMessage(id: string, text: string): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "agent_message",
    message_id: id,
    message: {
      kind: "agent_message",
      id,
      role: "agent",
      parts: [{ kind: "text", text }],
      created_at: "2026-05-23T00:00:00Z",
    },
  };
}

function agentContentMessage(id: string, content: Exclude<AgentMessagePart, { kind: "text" }>): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "agent_message",
    message_id: id,
    message: {
      kind: "agent_message",
      id,
      role: "agent",
      parts: [content],
      created_at: "2026-05-23T00:00:00Z",
    },
  };
}

function thoughtMessage(id: string, text: string): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "thought_message",
    message_id: id,
    message: {
      kind: "agent_message",
      id,
      role: "thought",
      parts: [{ kind: "text", text }],
      created_at: "2026-05-23T00:00:00Z",
    },
  };
}

function interruptionMessage(id: string, message: string, recoverable: boolean): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "interruption",
    message_id: id,
    message: {
      kind: "interruption",
      id,
      reason: "backend_unavailable",
      message,
      created_at: "2026-05-23T00:00:00Z",
      recoverable,
    },
  };
}

function toolActivity(name: string, title: string, details: ActivityToolDetails): ChatMessage {
  return {
    cursor: `cursor_${name}`,
    identity: `activity_${name}`,
    message_type: "activity",
    message_id: `activity_${name}`,
    message: {
      kind: "activity",
      id: `activity_${name}`,
      title,
      status: details.output?.success === false ? "error" : "completed",
      created_at: "2026-05-23T00:00:00Z",
      collapsed: false,
      steps: [
        {
          kind: "tool",
          name,
          status: details.output?.success === false ? "error" : "completed",
          input_summary: title,
          output_preview: details.output?.stdout,
          details,
        },
      ],
    },
  };
}

function input(overrides: Partial<NonNullable<ActivityToolDetails["input"]>> = {}): NonNullable<ActivityToolDetails["input"]> {
  return {
    command: [],
    fields: [],
    ...overrides,
  };
}

function emptyToolDetails(): ActivityToolDetails {
  return {
    locations: [],
    content: [],
    input: input(),
    output: undefined,
  };
}

function permissionMessage(id: string, command: string, options: PermissionOption[], kind = "execute"): ChatMessage {
  return {
    cursor: `cursor_${id}`,
    identity: id,
    message_type: "permission",
    message_id: id,
    message: {
      kind: "permission",
      id,
      request_id: `request_${id}`,
      title: command,
      tool_call: { id: "call_1", title: command, kind },
      state: "pending",
      created_at: "2026-05-23T00:00:00Z",
      options,
    },
  };
}
