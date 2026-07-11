import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskSnapshot } from "@openaide/app-shell-contracts";
import type { AgentOption } from "../state/composerOptions";
import { selectionWithProject } from "../state/composerOptions";
import { createInitialState } from "../state/store";
import { Composer } from "./Composer";
import { NewTaskView } from "./NewTaskView";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

describe("NewTaskView", () => {
  it("renders backend-provided project and agent choices in the context selectors", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    const agents: AgentOption[] = [
      { id: "codex", label: "Codex", description: "Code agent", icon: "openai", enabled: true },
      { id: "opencode", label: "OpenCode", description: "OpenCode agent", icon: "terminal", enabled: true },
    ];
    const tree = render(
      <NewTaskView
        agents={agents}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    const selectedAgentTrigger = buttonWithText(tree, "Codex");
    expect(selectedAgentTrigger.findAllByProps({ className: "agent-brand-icon openai-agent-icon" })).toHaveLength(1);

    act(() => buttonWithText(tree, "OpenAIDE").props.onClick());
    expect(menuLabels(tree)).toContain("OpenAIDE");

    act(() => buttonWithText(tree, "Codex").props.onClick());
    expect(menuLabels(tree)).toEqual(expect.arrayContaining(["Codex", "OpenCode"]));
  });

  it("keeps project and agent menus populated during transient App Server state gaps", () => {
    const state = createInitialState();
    state.tasks = [{
      agent_id: "codex",
      agent_name: "Codex",
      created_at: "2026-05-22T00:00:00.000Z",
      isolation: "local",
      last_activity: "2026-05-22T00:00:00.000Z",
      message_history_version: 1,
      has_messages: true,
      project_id: "project_1",
      project_label: "OpenAIDE",
      status: "inactive",
      task_id: "task_1",
      task_version: 1,
      title: "Existing task",
      unread: false,
      updated_at: "2026-05-22T00:00:00.000Z",
      workspace_root: "",
    }];
    state.newTask.selection = {
      ...state.newTask.selection,
      projectId: "project_1",
      workspaceLabel: "OpenAIDE",
    };
    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    act(() => buttonWithText(tree, "OpenAIDE").props.onClick());
    expect(menuLabels(tree)).toContain("OpenAIDE");

    act(() => buttonWithText(tree, "Codex").props.onClick());
    expect(menuLabels(tree).length).toBeGreaterThan(0);
  });

  it("shows project loading instead of a final empty project selector before App Server state arrives", () => {
    const state = createInitialState();
    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        loadingProjects
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    expect(buttonWithText(tree, "Loading").props.disabled).toBe(true);
    expect(textContent(tree)).toContain("Loading projects.");
  });

  it("accepts a workspace path when no project has been seen before", () => {
    const state = createInitialState();
    state.workspaceRootsLoaded = true;
    const dispatch = vi.fn();
    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={dispatch}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    act(() => buttonWithText(tree, "Choose workspace").props.onClick());
    act(() => tree.root.findByProps({ id: "new-task-workspace-root" }).props.onChange({ target: { value: "/workspace/new-app" } }));
    act(() => tree.root.findByProps({ "aria-label": "Use workspace path" }).props.onClick());

    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:workspace",
      workspace: {
        path: "/workspace/new-app",
        label: "new-app",
        projectId: "project-fe42cc83da346a18",
      },
    });
  });

  it("accepts a workspace selected from the App Server folder picker", async () => {
    const state = createInitialState();
    state.workspaceRootsLoaded = true;
    const dispatch = vi.fn();
    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={dispatch}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
        workspaceBrowser={workspaceBrowserCallbacks()}
      />,
    );

    act(() => buttonWithText(tree, "Choose workspace").props.onClick());
    await act(async () => {
      await Promise.resolve();
    });
    act(() => buttonWithText(tree, "Workspace").props.onClick());
    await act(async () => {
      await Promise.resolve();
    });
    act(() => buttonWithText(tree, "new-app").props.onClick());
    await act(async () => {
      await Promise.resolve();
    });
    act(() => buttonWithText(tree, "Use this folder").props.onClick());

    expect(dispatch).toHaveBeenCalledWith({
      type: "newTask:workspace",
      workspace: {
        path: "/workspace/new-app",
        label: "new-app",
        projectId: "project-fe42cc83da346a18",
      },
    });
  });

  it("allows sending after a new workspace path is selected", () => {
    const state = createInitialState();
    state.workspaceRootsLoaded = true;
    state.newTask.selection = {
      ...state.newTask.selection,
      projectId: "project-fe42cc83da346a18",
      workspaceRoot: "/workspace/new-app",
      workspaceLabel: "new-app",
    };
    state.newTask.prompt = "Start in a new workspace";
    const onSubmitTask = vi.fn();

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={onSubmitTask}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    const send = tree.root.findByProps({ "aria-label": "Send message" });
    expect(send.props.disabled).toBe(false);

    act(() => send.props.onClick());

    expect(onSubmitTask).toHaveBeenCalledWith({ prompt: "Start in a new workspace", context: [] });
  });

  it("focuses the prompt composer when the new-task surface opens", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    expect(tree.root.findByType(Composer).props.autoFocus).toBe(true);
  });

  it("blocks submit while App Server project state is still loading", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    const onSubmitTask = vi.fn();
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };
    state.newTask.prompt = "Start too early";

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        loadingProjects
        onSelectConfigOption={vi.fn()}
        onSubmitTask={onSubmitTask}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    const send = tree.root.findByProps({ "aria-label": "Send message" });
    expect(send.props.disabled).toBe(true);

    act(() => send.props.onClick());

    expect(onSubmitTask).not.toHaveBeenCalled();
  });

  it("allows starting a typed task while agent config options are still loading", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    const onSubmitTask = vi.fn();
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = undefined;
    state.newTask.configOptionsLoading = true;
    state.newTask.prompt = "Start without waiting for optional model options";

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={onSubmitTask}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    const send = tree.root.findByProps({ "aria-label": "Send message" });
    expect(send.props.disabled).toBe(false);

    act(() => send.props.onClick());

    expect(onSubmitTask).toHaveBeenCalledTimes(1);
    expect(textContent(tree)).toContain("Preparing Codex options");
  });

  it("keeps typed new-task text local until submit", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    const dispatch = vi.fn();
    const onSubmitTask = vi.fn();
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={dispatch}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={onSubmitTask}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    act(() => tree.root.findByType(Composer).props.onChange("Fix the typing lag"));

    expect(dispatch).not.toHaveBeenCalledWith({ type: "prompt", prompt: "Fix the typing lag" });

    act(() => tree.root.findByProps({ "aria-label": "Send message" }).props.onClick());

    expect(onSubmitTask).toHaveBeenCalledWith({ prompt: "Fix the typing lag", context: [] });
  });

  it("keeps file attachment actions disabled while project state is still loading", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        fileBrowser={fileBrowserCallbacks()}
        loadingProjects
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    act(() => tree.root.findByProps({ "aria-label": "Add context" }).props.onClick());

    expect(menuButtonByStrongLabel(tree, "Project files").props.disabled).toBe(true);
    expect(menuButtonByStrongLabel(tree, "Upload or photo").props.disabled).toBe(true);
    expect(tree.root.findAllByProps({ type: "file" })[0].props.disabled).toBe(true);
  });

  it("enables only App Server-backed attachment actions when project context is ready", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        fileBrowser={fileBrowserCallbacks()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    act(() => tree.root.findByProps({ "aria-label": "Add context" }).props.onClick());

    expect(menuButtonByStrongLabel(tree, "Project files").props.disabled).toBeFalsy();
    expect(menuButtonByStrongLabel(tree, "Upload or photo").props.disabled).toBeFalsy();
    expect(tree.root.findAllByProps({ type: "file" })[0].props.disabled).toBeFalsy();
  });

  it("renders prepared Task image previews without a visible file name", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };
    state.snapshot = taskSnapshot("task_1", false);
    state.taskInputs.task_1 = {
      prompt: "Explain this",
      context: [{
        kind: "file",
        label: "pasted.png",
        local_id: "attachment_1",
        app_server_handle_id: "attachment-handle-image" as never,
        preview_url: "data:image/png;base64,AQID",
      }],
    };

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    expect(textContent(tree)).not.toContain("pasted.png");
    expect(tree.root.findByProps({ className: "composer-image-preview" }).props.src).toBe("data:image/png;base64,AQID");
    expect(editorHtml(tree)).toBe("Explain this");
  });

  it("explains that attachments need a message before sending", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };
    state.newTask.context = [{
      kind: "file",
      label: "README.md",
      local_id: "attachment_1",
      app_server_handle_id: "attachment-handle-readme" as never,
    }];

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    expect(textContent(tree)).toContain("Add a message for this Agent.");
  });

  it("does not show a text-required error while attachment-only capability is loading", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };
    state.taskInputs.task_1 = {
      context: [{
        kind: "file",
        label: "pasted.png",
        local_id: "attachment_1",
        app_server_handle_id: "attachment-handle-image" as never,
        preview_url: "data:image/png;base64,AQID",
      }],
      prompt: "",
    };
    state.snapshot = {
      ...taskSnapshot("task_1", false),
      send_capability: { state: "loading", attachment_only: false },
    };

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    expect(textContent(tree)).not.toContain("Add a message for this Agent.");
  });

  it("sends an attachment-only image when the prepared Task supports it", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };
    state.newTask.context = [{
      kind: "file",
      label: "pasted.png",
      local_id: "attachment_1",
      app_server_handle_id: "attachment-handle-image" as never,
      preview_url: "data:image/png;base64,AQID",
    }];
    state.snapshot = {
      ...taskSnapshot("task_1", false),
      send_capability: { state: "ready", attachment_only: true },
    };
    const onSubmitTask = vi.fn();

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={onSubmitTask}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    const send = tree.root.findByProps({ "aria-label": "Send message" });
    expect(send.props.disabled).toBe(false);

    act(() => send.props.onClick());

    expect(onSubmitTask).toHaveBeenCalledWith({ prompt: "", context: state.newTask.context });
  });

  it("requests composer focus when a typed draft becomes sendable after project state loads", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.prompt = "Ready after options";
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        loadingProjects
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="enter"
      />,
    );
    const waitingFocusKey = tree.root.findByType(Composer).props.focusRequestKey;

    act(() => {
      tree.update(
        <NewTaskView
          agents={[]}
          dispatch={vi.fn()}
          onSelectConfigOption={vi.fn()}
          onSubmitTask={vi.fn()}
          resetOptionsRequestKey={vi.fn()}
          state={state}
          submitShortcut="enter"
        />,
      );
    });

    expect(waitingFocusKey).toBe("0:waiting");
    expect(tree.root.findByType(Composer).props.focusRequestKey).toBe("0:ready");
  });

  it("moves a submitted new task into the normal chat layout while startup is pending", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };
    state.newTask.prompt = "";
    state.newTask.context = [];
    state.newTask.pending = {
      prompt: "Do not erase this",
      context: [{
        kind: "file",
        label: "pasted.png",
        local_id: "attachment_1",
        app_server_handle_id: "attachment-handle-image" as never,
        preview_url: "data:image/png;base64,AQID",
      }],
    };
    state.newTask.submitting = true;

    const onCancelTask = vi.fn();
    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onCancelTask={onCancelTask}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    expect(editorHtml(tree)).toBe("");
    expect(composerEditor(tree).props["aria-disabled"]).toBe(true);
    expect(composerEditor(tree).props["data-placeholder"]).toBe("");
    expect(tree.root.findAllByProps({ "aria-label": "Send message" })).toHaveLength(0);
    expect(tree.root.findByProps({ "aria-label": "Task status: Starting" })).toBeTruthy();
    expect(tree.root.findByProps({ className: "new-task-starting-status" })).toBeTruthy();
    expect(tree.root.findAllByProps({ className: "working-status-dots" })).toHaveLength(0);
    expect(textContent(tree)).toContain("Do not erase this");
    expect(tree.root.findByProps({ className: "chat-user" })).toBeTruthy();
    expect(textContent(tree)).not.toContain("pasted.png");
    expect(tree.root.findByProps({ className: "chat-image-grid" }).props["data-layout"]).toBe("single");
    expect(tree.root.findByProps({ className: "chat-image-attachment" })).toBeTruthy();
    expect(tree.root.findByProps({ className: "chat-image-preview" }).props.src).toBe("data:image/png;base64,AQID");
    const submittedMessage = tree.root.findByProps({ "aria-label": "Submitted message" });
    expect(submittedMessage
      .findAll((node) => node.props.className === "chat-image-grid" || node.props.className === "chat-user")
      .map((node) => node.props.className))
      .toEqual(["chat-image-grid", "chat-user"]);
    expect(tree.root.findByProps({ className: "new-task-starting-status" }).findByType("span").children.join("")).toBe("Starting task…");
    act(() => tree.root.findByProps({ "aria-label": "Stop task" }).props.onClick());
    expect(onCancelTask).toHaveBeenCalledOnce();
    expect(textContent(tree)).not.toContain("What are we working on?");
    expect(tree.root.findAllByProps({ className: "new-task-context-controls" })).toHaveLength(0);
  });

  it("labels native-session adoption as opening a task", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };
    state.newTask.submitting = true;
    state.newTask.nativeSessions = {
      adoptingSessionId: "session_1",
      items: [],
      loaded: true,
      loading: false,
    };

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    expect(tree.root.findByProps({ "aria-label": "Opening task" })).toBeTruthy();
    expect(textContent(tree)).toContain("Opening task");
    expect(textContent(tree)).not.toContain("Starting task");
  });

  it("falls back to the new-task draft when an empty prepared Task has no local input yet", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };
    state.newTask.prompt = "Still typing";
    state.snapshot = taskSnapshot("task_1", false);

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    expect(editorHtml(tree)).toBe("Still typing");
  });

  it("shows commands from the prepared empty Task when the user types slash", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = { agent_id: "codex", options: [], status: "ready" };
    state.snapshot = {
      ...taskSnapshot("task_1", false),
      agent_commands: {
        agent_id: "codex",
        commands: [{ name: "review", description: "Review changes." }],
        status: "ready",
      },
    };

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    act(() => composerEditor(tree).props.onInput({
      currentTarget: {
        innerText: "/",
        ownerDocument: { getSelection: () => null },
        textContent: "/",
        toggleAttribute: vi.fn(),
      },
    }));

    const picker = tree.root.findByProps({ role: "listbox", "aria-label": "Slash commands" });
    expect(nodeText(picker)).toContain("/review");
  });

  it("keeps already loaded new-task options visible and ordered while the prepared Task options catch up", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = {
      agent_id: "codex",
      options: [
        {
          category: "mode",
          current_value: "agent",
          id: "mode",
          label: "Mode",
          values: [{ id: "agent", label: "Agent" }],
        },
        {
          category: "model",
          current_value: "gpt-5.5",
          id: "model",
          label: "Model",
          values: [{ id: "gpt-5.5", label: "GPT-5.5" }],
        },
        {
          category: "thought_level",
          current_value: "medium",
          id: "reasoning_effort",
          label: "Reasoning",
          values: [{ id: "medium", label: "Medium" }],
        },
        {
          current_value: "off",
          id: "fast-mode",
          label: "Fast mode",
          values: [{ id: "off", label: "Off" }],
        },
      ],
      status: "ready",
    };
    state.newTask.pending = {
      prompt: "Pending send",
      context: [],
      configOptions: state.newTask.configOptions,
    };
    state.newTask.configOptions = undefined;
    state.newTask.submitting = true;
    state.snapshot = {
      ...taskSnapshot("task_1", false),
      agent_config: {
        agent_id: "codex",
        options: [
          {
            current_value: "off",
            id: "fast-mode",
            label: "fast-mode",
            values: [{ id: "off", label: "Off" }],
          },
          {
            category: "mode",
            current_value: "agent",
            id: "mode",
            label: "mode",
            values: [{ id: "agent", label: "Agent" }],
          },
          {
            category: "thought_level",
            current_value: "medium",
            id: "reasoning_effort",
            label: "reasoning_effort",
            values: [{ id: "medium", label: "Medium" }],
          },
          {
            category: "model",
            current_value: "gpt-5.5",
            id: "model",
            label: "model",
            values: [{ id: "gpt-5.5", label: "GPT-5.5" }],
          },
        ],
        status: "ready",
      },
    };

    const tree = render(
      <NewTaskView
        agents={[]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    expect(composerButtonLabels(tree)).toEqual([
      "Agent",
      "GPT-5.5",
      "Medium",
      "Fast: Off",
      "Options · GPT-5.5",
    ]);
    expect(() => buttonWithText(tree, "fast-mode: Off")).toThrow();
  });

  it("dismisses context selector menus on Escape and before composer menu interactions", () => {
    const state = createInitialState();
    const project = { projectId: "project_1", label: "OpenAIDE" };
    state.projects = [project];
    state.newTask.selection = selectionWithProject(state.newTask.selection, project);
    state.newTask.configOptions = {
      agent_id: "codex",
      options: [{
        category: "model",
        current_value: "gpt-5.5",
        id: "model",
        label: "Model",
        values: [{ id: "gpt-5.5", label: "GPT-5.5" }],
      }],
      status: "ready",
    };
    const tree = render(
      <NewTaskView
        agents={[{ id: "codex", label: "Codex", description: "Code agent", icon: "openai", enabled: true }]}
        dispatch={vi.fn()}
        onSelectConfigOption={vi.fn()}
        onSubmitTask={vi.fn()}
        resetOptionsRequestKey={vi.fn()}
        state={state}
        submitShortcut="mod_enter"
      />,
    );

    act(() => buttonWithText(tree, "Codex").props.onClick());
    expect(menuLabels(tree)).toContain("Codex");

    act(() => tree.root.findByProps({ "aria-label": "New task" }).props.onKeyDown({ key: "Escape" }));
    expect(menuLabels(tree)).not.toContain("Codex");

    act(() => buttonWithText(tree, "Codex").props.onClick());
    act(() =>
      tree.root.findByProps({ "aria-label": "New task" }).props.onPointerDownCapture({
        target: {},
      }),
    );
    act(() => buttonWithText(tree, "GPT-5.5").props.onClick());

    expect(menuLabels(tree)).not.toContain("Codex");
    expect(menuLabels(tree)).toContain("GPT-5.5");
  });
});

const editorDomByTree = new WeakMap<object, HTMLElement>();

function render(element: React.ReactElement) {
  const editorDom = mockEditorDom();
  let tree: ReturnType<typeof create> | undefined;
  act(() => {
    tree = create(element, {
      createNodeMock: (node) =>
        (node.props as { className?: string }).className === "composer-editor" ? editorDom : null,
    });
  });
  editorDomByTree.set(tree!, editorDom);
  return tree!;
}

function mockEditorDom() {
  let html = "";
  const editor = {
    focus: vi.fn(),
    innerText: "",
    ownerDocument: {
      activeElement: undefined,
      getSelection: () => null,
    },
    textContent: "",
  };
  Object.defineProperty(editor, "innerHTML", {
    get: () => html,
    set: (value: string) => {
      html = value;
      const text = value.replace(/<br>/g, "\n").replace(/<[^>]+>/g, "");
      editor.innerText = text;
      editor.textContent = text;
    },
  });
  return editor as unknown as HTMLElement;
}

function buttonWithText(tree: ReturnType<typeof render>, text: string) {
  const button = tree.root.findAllByType("button").find((node) =>
    nodeText(node).includes(text),
  );
  if (!button) throw new Error(`Button not found: ${text}`);
  return button;
}

function nodeText(node: ReturnType<typeof render>["root"]): string {
  return node.children.map((child) => {
    if (typeof child === "string") return child;
    if (typeof child === "object" && child !== null && "children" in child) return nodeText(child as typeof node);
    return "";
  }).join(" ");
}

function menuLabels(tree: ReturnType<typeof render>) {
  return tree.root
    .findAllByType("strong")
    .map((node) => node.children.join(""));
}

function composerButtonLabels(tree: ReturnType<typeof render>) {
  return tree.root.findByType(Composer)
    .findAllByType("button")
    .map((button) =>
      button.children
        .filter((child): child is string => typeof child === "string")
        .join(""),
    )
    .filter(Boolean);
}

function textContent(tree: ReturnType<typeof render>) {
  return tree.root.findAll((node) => typeof node.type === "string")
    .flatMap((node) => node.children)
    .filter((child): child is string => typeof child === "string")
    .join(" ");
}

function composerEditor(tree: ReturnType<typeof render>) {
  return tree.root.findByProps({ className: "composer-editor" });
}

function editorHtml(tree: ReturnType<typeof render>) {
  return editorDomByTree.get(tree)?.innerHTML ?? "";
}

function menuButtonByStrongLabel(tree: ReturnType<typeof render>, label: string) {
  const button = tree.root.findAllByType("button").find((node) =>
    node.findAllByType("strong").some((strong) => strong.children.join("") === label),
  );
  if (!button) throw new Error(`Menu button not found: ${label}`);
  return button;
}

function menuButtonsByStrongLabel(tree: ReturnType<typeof render>, label: string) {
  return tree.root.findAllByType("button").filter((node) =>
    node.findAllByType("strong").some((strong) => strong.children.join("") === label),
  );
}

function fileBrowserCallbacks(): TaskFileBrowserCallbacks {
  return {
    attachEmbedded: vi.fn(async () => undefined),
    attachFileReference: vi.fn(async () => undefined),
    attachPastedImage: vi.fn(async () => undefined),
    listDirectory: vi.fn(async () => ({ directory: { label: "Workspace", rootId: "root-1" as never }, entries: [] })),
    listRoots: vi.fn(async () => [{ label: "Workspace", rootId: "root-1" as never }]),
  };
}

function workspaceBrowserCallbacks() {
  return {
    listRoots: vi.fn(async () => [{ label: "Workspace", path: "/workspace" }]),
    listDirectory: vi.fn(async (path: string) => {
      if (path === "/workspace") {
        return {
          directory: { label: "Workspace", path: "/workspace", parentPath: "/" },
          entries: [{ label: "new-app", path: "/workspace/new-app" }],
        };
      }
      return {
        directory: { label: "new-app", path: "/workspace/new-app", parentPath: "/workspace" },
        entries: [],
      };
    }),
  };
}

function taskSnapshot(taskId: string, hasMessages: boolean): TaskSnapshot {
  return {
    task: {
      task_id: taskId,
      title: "New task",
      status: "inactive",
      task_version: 1,
      message_history_version: 1,
      has_messages: hasMessages,
      unread: false,
      created_at: "2026-05-22T00:00:00.000Z",
      updated_at: "2026-05-22T00:00:00.000Z",
      last_activity: "2026-05-22T00:00:00.000Z",
      agent_id: "codex",
      agent_name: "Codex",
      isolation: "local",
      workspace_root: "/workspace",
      project_id: "project_1",
      project_label: "OpenAIDE",
    },
    chat: {
      task_id: taskId,
      items: [],
      has_before: false,
      has_messages: hasMessages,
      total_count: 0,
      version: 1,
    },
    permissions: [],
    settings_summary: {
      agent_id: "codex",
      isolation: "local",
      config_options: {},
    },
    send_capability: { state: "ready", attachment_only: false },
    revision: 1,
  };
}
