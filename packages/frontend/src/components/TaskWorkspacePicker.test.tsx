import { act, create, type ReactTestInstance } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WorktreeRepositorySnapshot, WorktreeSummary } from "@openaide/app-server-client";
import { TaskWorkspacePicker } from "./TaskWorkspacePicker";
import type { NewTaskViewIntents } from "./NewTaskView";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

describe("TaskWorkspacePicker", () => {
  it("leaves desktop Worktree Management when Back is pressed from a selected worktree", () => {
    const onClose = vi.fn();
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <TaskWorkspacePicker
          initialMode="manage"
          intents={testIntents()}
          managementOnly
          onClose={onClose}
          project={{ projectId: "project_1", label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE", worktreeRepositoryId: "repository_1", projectWorktreeId: "worktree_root" }}
          repository={repository()}
          tasks={[]}
        />,
      );
    });

    act(() => tree.root.findByProps({ "aria-label": "Back" }).props.onClick());

    expect(onClose).toHaveBeenCalledOnce();
  });

  it("selects Project root and reusable worktrees by opaque identity", () => {
    const intents = testIntents();
    const tree = render(intents);
    const options = tree.root.findAllByProps({ role: "option" });

    expect(options.map(text)).toEqual(expect.arrayContaining([expect.stringContaining("Project root"), expect.stringContaining("Sidebar scrolling")]));
    act(() => options.find((option) => text(option).includes("Sidebar scrolling"))?.props.onClick());

    expect(intents.selectWorktree).toHaveBeenCalledWith({
      worktreeId: "worktree_sidebar",
      label: "Sidebar scrolling",
      path: "/workspace/OpenAIDE-sidebar",
    });
  });

  it("reveals an unavailable worktree reason when its row is activated", () => {
    const intents = testIntents();
    const repo = repository();
    repo.worktrees[1] = worktree({
      ...repo.worktrees[1],
      availability: "unavailable",
      availabilityReason: "The worktree folder is missing.",
    });
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <TaskWorkspacePicker
          intents={intents}
          onClose={vi.fn()}
          project={{ projectId: "project_1", label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE", worktreeRepositoryId: "repository_1", projectWorktreeId: "worktree_root" }}
          repository={repo}
          tasks={[]}
        />,
      );
    });

    const option = tree.root.findAllByProps({ role: "option" }).find((item) => text(item).includes("Sidebar scrolling"));
    expect(option?.props.disabled).toBeUndefined();
    expect(option?.props["aria-disabled"]).toBeUndefined();
    expect(option?.props["aria-label"]).toBe("Sidebar scrolling, unavailable. Show reason");
    act(() => option?.props.onClick());

    expect(text(tree.root.findByProps({ className: "task-workspace-option-reason" })))
      .toContain("The worktree folder is missing.");
    expect(intents.selectWorktree).not.toHaveBeenCalled();
  });

  it("uses the configured Project worktree as Project root even when Git primary is elsewhere", () => {
    const intents = testIntents();
    const repo = repository();
    repo.worktrees[0] = worktree({
      worktreeId: "worktree_primary" as never,
      isMain: true,
      name: "Primary checkout",
      path: "/workspace/OpenAIDE-primary",
      head: { kind: "branch", name: "main", commit: "8ea7d1c000000000" },
    });
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <TaskWorkspacePicker
          intents={intents}
          onClose={vi.fn()}
          project={{
            projectId: "project_1",
            label: "OpenAIDE",
            workspaceRoot: "/workspace/OpenAIDE-sidebar",
            available: true,
            worktreeRepositoryId: "repository_1",
            projectWorktreeId: "worktree_sidebar",
          }}
          repository={repo}
          selectedWorktreeId={undefined}
          tasks={[]}
        />,
      );
    });

    const options = tree.root.findAllByProps({ role: "option" });
    const projectRoot = options.find((option) => text(option).includes("Project root"));
    const primary = options.find((option) => text(option).includes("Primary checkout"));
    expect(projectRoot?.props["aria-selected"]).toBe(true);
    expect(primary).toBeDefined();
    act(() => primary?.props.onClick());
    expect(intents.selectWorktree).toHaveBeenLastCalledWith({
      worktreeId: "worktree_primary",
      label: "Primary checkout",
      path: "/workspace/OpenAIDE-primary",
    });
  });

  it("derives a branch from the name until the branch is edited manually", () => {
    const intents = testIntents();
    const tree = render(intents);
    act(() => tree.root.findAllByType("button").find((button) => hasText(button, "New worktree"))?.props.onClick());

    const name = tree.root.findAllByType("input")[0];
    const branchToggle = tree.root.findAllByType("input").find((input) => input.props.type === "checkbox");
    act(() => name.props.onChange({ target: { value: "Fix sidebar scroll" } }));
    act(() => branchToggle?.props.onChange({ target: { checked: true } }));
    const branch = tree.root.findAllByType("input").find((input) => input.props.type !== "checkbox" && input !== name);
    expect(branch?.props.value).toBe("fix-sidebar-scroll");

    act(() => branch?.props.onChange({ target: { value: "custom/sidebar" } }));
    act(() => name.props.onChange({ target: { value: "Different title" } }));
    expect(tree.root.findAllByType("input").find((input) => input.props.value === "custom/sidebar")).toBeDefined();
  });

  it("adds a visible suffix when the generated branch already exists", () => {
    const intents = testIntents();
    const repo = repository();
    repo.bases?.push({ kind: "localBranch", name: "fix-sidebar-scroll", commit: "8ea7d1c000000000" });
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <TaskWorkspacePicker
          intents={intents}
          onClose={vi.fn()}
          project={{ projectId: "project_1", label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE", worktreeRepositoryId: "repository_1", projectWorktreeId: "worktree_root" }}
          repository={repo}
          tasks={[]}
        />,
      );
    });
    act(() => tree.root.findAllByType("button").find((button) => hasText(button, "New worktree"))?.props.onClick());
    const name = tree.root.findAllByType("input")[0];
    act(() => name.props.onChange({ target: { value: "Fix sidebar scroll" } }));
    act(() => tree.root.findAllByType("input").find((input) => input.props.type === "checkbox")?.props.onChange({ target: { checked: true } }));

    expect(tree.root.findAllByType("input").some((input) => input.props.value === "fix-sidebar-scroll-2")).toBe(true);
  });

  it("offers Forget for an unavailable worktree and removes it from active inventory", async () => {
    const intents = testIntents();
    const repo = repository();
    repo.worktrees[1] = worktree({
      ...repo.worktrees[1],
      availability: "unavailable",
      availabilityReason: "Git no longer lists this worktree",
      linkedTaskCount: 1,
    });
    repo.worktrees.push(worktree({
      worktreeId: "worktree_forgotten" as never,
      name: "Old workspace",
      path: "/workspace/old",
      forgotten: true,
    }));
    vi.mocked(intents.removalPreflight).mockResolvedValue({
      status: "safe",
      blockers: [],
      ownership: "external",
      path: "/workspace/OpenAIDE-sidebar",
      ignoredFilesWillBeRemoved: false,
    });
    let tree!: ReturnType<typeof create>;
    await act(async () => {
      tree = create(
        <TaskWorkspacePicker
          initialMode="manage"
          intents={intents}
          managementOnly
          onClose={vi.fn()}
          project={{ projectId: "project_1", label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE", worktreeRepositoryId: "repository_1", projectWorktreeId: "worktree_root" }}
          repository={repo}
          tasks={[linkedTask()]}
        />,
      );
    });

    expect(text(tree.root)).not.toContain("Old workspace");
    act(() => tree.root.findAllByType("button").find((button) => text(button).includes("Sidebar scrolling"))?.props.onClick());
    expect(text(tree.root)).not.toContain("New task here");
    await act(async () => {
      await tree.root.findAllByType("button").find((button) => hasText(button, "Forget worktree…"))?.props.onClick();
    });

    expect(text(tree.root)).toContain("Forget “Sidebar scrolling”?");
    expect(text(tree.root)).toContain("1 linked Task will remain readable");
    expect(text(tree.root)).toContain("The folder is already missing");
  });

  it("preserves linked Tasks and falls New Task back to Project root after removal", async () => {
    const intents = testIntents();
    vi.mocked(intents.removalPreflight).mockResolvedValue({
      status: "safe",
      blockers: [],
      ownership: "external",
      path: "/workspace/OpenAIDE-sidebar",
      ignoredFilesWillBeRemoved: true,
    });
    vi.mocked(intents.removeWorktree).mockResolvedValue(undefined);
    let tree!: ReturnType<typeof create>;
    await act(async () => {
      tree = create(
        <TaskWorkspacePicker
          initialMode="manage"
          intents={intents}
          managementOnly
          onClose={vi.fn()}
          project={{ projectId: "project_1", label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE", worktreeRepositoryId: "repository_1", projectWorktreeId: "worktree_root" }}
          repository={repository()}
          selectedWorktreeId="worktree_sidebar"
          tasks={[linkedTask()]}
        />,
      );
    });
    act(() => tree.root.findAllByType("button").find((button) => text(button).includes("Sidebar scrolling"))?.props.onClick());
    await act(async () => {
      await tree.root.findAllByType("button").find((button) => hasText(button, "Remove worktree…"))?.props.onClick();
    });

    expect(text(tree.root)).toContain("1 linked Task will remain readable");
    expect(text(tree.root)).toContain("Branch fix/sidebar-scroll will be kept");
    await act(async () => {
      await tree.root.findAllByType("button").find((button) => hasText(button, "Remove worktree") && !hasText(button, "…"))?.props.onClick();
    });

    expect(intents.removeWorktree).toHaveBeenCalledWith("repository_1", "worktree_sidebar");
    expect(intents.selectWorktree).toHaveBeenCalledWith({
      worktreeId: undefined,
      label: "Project root",
      path: "/workspace/OpenAIDE",
    });
  });
});

function render(intents: NewTaskViewIntents) {
  let tree!: ReturnType<typeof create>;
  act(() => {
    tree = create(
      <TaskWorkspacePicker
        intents={intents}
        onClose={vi.fn()}
        project={{
          projectId: "project_1",
          label: "OpenAIDE",
          workspaceRoot: "/workspace/OpenAIDE",
          available: true,
          worktreeRepositoryId: "repository_1",
          projectWorktreeId: "worktree_root",
        }}
        repository={repository()}
        tasks={[]}
      />,
    );
  });
  return tree;
}

function testIntents(): NewTaskViewIntents {
  return {
    changePrompt: vi.fn(),
    reportAttachmentError: vi.fn(),
    selectAgent: vi.fn(),
    selectIsolation: vi.fn(),
    selectProject: vi.fn(),
    selectWorkspace: vi.fn(),
    selectWorktree: vi.fn(),
    refreshWorktrees: vi.fn(),
    createWorktree: vi.fn(),
    recreateWorktree: vi.fn(),
    removeWorktree: vi.fn(),
    removalPreflight: vi.fn(),
    renameWorktree: vi.fn(),
    openFolder: vi.fn(),
    openTask: vi.fn(),
  };
}

function repository(): WorktreeRepositorySnapshot {
  return {
    repositoryId: "repository_1" as never,
    revision: 1,
    bases: [{ kind: "head", commit: "8ea7d1c000000000", label: "Current HEAD" }],
    worktrees: [
      worktree({
        worktreeId: "worktree_root" as never,
        isMain: true,
        name: "OpenAIDE",
        path: "/workspace/OpenAIDE",
        head: { kind: "branch", name: "main", commit: "8ea7d1c000000000" },
      }),
      worktree({
        worktreeId: "worktree_sidebar" as never,
        name: "Sidebar scrolling",
        path: "/workspace/OpenAIDE-sidebar",
        head: { kind: "branch", name: "fix/sidebar-scroll", commit: "8ea7d1c000000000" },
        linkedTaskCount: 1,
      }),
    ],
  };
}

function worktree(overrides: Partial<WorktreeSummary>): WorktreeSummary {
  return {
    worktreeId: "worktree_1" as never,
    name: "Worktree",
    path: "/workspace/worktree",
    forgotten: false,
    ownership: "external",
    isMain: false,
    head: { kind: "detached", commit: "8ea7d1c000000000" },
    availability: "available",
    projectIds: [],
    linkedTaskCount: 0,
    runningTaskCount: 0,
    ...overrides,
  };
}

function linkedTask() {
  return {
    task_id: "task_1",
    project_id: "project_1",
    agent_id: "codex",
    agent_name: "Codex",
    title: "Fix sidebar scrolling",
    status: "inactive" as const,
    task_version: 1,
    message_history_version: 1,
    has_messages: true,
    created_at: "1",
    updated_at: "1",
    last_activity: "1",
    unread: false,
    workspace_root: "/workspace/OpenAIDE-sidebar",
    isolation: "git_worktree" as const,
    workspace_available: true,
    worktree_id: "worktree_sidebar",
  };
}

function text(node: ReactTestInstance) {
  return node.findAll((candidate) => typeof candidate.children[0] === "string")
    .flatMap((candidate) => candidate.children.filter((child): child is string => typeof child === "string"))
    .join("");
}

function hasText(node: ReactTestInstance, value: string) {
  return node.children.some((child) => child === value);
}
