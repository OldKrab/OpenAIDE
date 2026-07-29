import { act, create, type ReactTestInstance } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WorktreeRepositorySnapshot, WorktreeSummary } from "@openaide/app-server-client";
import { TaskWorkspacePicker } from "./TaskWorkspacePicker";
import type { NewTaskViewIntents } from "./NewTaskView";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

describe("TaskWorkspacePicker", () => {
  it("leaves the New Task popup to manage worktrees in Settings", () => {
    const onManageWorktrees = vi.fn();
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <TaskWorkspacePicker
          intents={testIntents()}
          onClose={vi.fn()}
          onManageWorktrees={onManageWorktrees}
          project={{ projectId: "project_1", label: "OpenAIDE", workspaceRoot: "/workspace/OpenAIDE", worktreeRepositoryId: "repository_1", projectWorktreeId: "worktree_root" }}
          repository={repository()}
          tasks={[]}
        />,
      );
    });

    act(() => tree.root.findAllByType("button")
      .find((button) => hasText(button, "Manage worktrees"))?.props.onClick());

    expect(onManageWorktrees).toHaveBeenCalledWith("project_1");
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

  it("does not report the branch being created as a collision while creation is in progress", async () => {
    const intents = testIntents();
    const repo = repository();
    vi.mocked(intents.createWorktree).mockImplementation((_project, _draft, onProgress) => {
      onProgress?.({
        operationId: "operation_1",
        kind: "create",
        state: "queued",
        stage: "Waiting to start",
      } as never);
      return new Promise(() => {});
    });
    const project = {
      projectId: "project_1",
      label: "OpenAIDE",
      workspaceRoot: "/workspace/OpenAIDE",
      worktreeRepositoryId: "repository_1",
      projectWorktreeId: "worktree_root",
    };
    const view = () => (
      <TaskWorkspacePicker
        intents={intents}
        onClose={vi.fn()}
        project={project}
        repository={repo}
        tasks={[]}
      />
    );
    let tree!: ReturnType<typeof create>;
    act(() => { tree = create(view()); });
    act(() => tree.root.findAllByType("button").find((button) => hasText(button, "New worktree"))?.props.onClick());
    const name = tree.root.findAllByType("input")[0];
    act(() => name.props.onChange({ target: { value: "Rename task" } }));
    act(() => tree.root.findAllByType("input").find((input) => input.props.type === "checkbox")?.props.onChange({ target: { checked: true } }));
    await act(async () => {
      void tree.root.findAllByType("button").find((button) => hasText(button, "Create worktree"))?.props.onClick();
      await Promise.resolve();
    });

    repo.bases?.push({ kind: "localBranch", name: "rename-task", commit: "8ea7d1c000000000" });
    act(() => tree.update(view()));

    expect(text(tree.root)).not.toContain("A local branch with this name already exists.");
  });

  it("presents the initial queued state as startup rather than lock contention", async () => {
    const intents = testIntents();
    vi.mocked(intents.createWorktree).mockImplementation((_project, _draft, onProgress) => {
      onProgress?.({
        operationId: "operation_1",
        kind: "create",
        state: "queued",
        stage: "Waiting to start",
      } as never);
      return new Promise(() => {});
    });
    const tree = render(intents);
    act(() => tree.root.findAllByType("button").find((button) => hasText(button, "New worktree"))?.props.onClick());
    act(() => tree.root.findAllByType("input")[0].props.onChange({ target: { value: "Rename task" } }));
    await act(async () => {
      void tree.root.findAllByType("button").find((button) => hasText(button, "Create worktree"))?.props.onClick();
      await Promise.resolve();
    });

    expect(text(tree.root)).toContain("Starting worktree operation");
    expect(text(tree.root)).not.toContain("Waiting for another worktree operation.");
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

function text(node: ReactTestInstance) {
  return node.findAll((candidate) => typeof candidate.children[0] === "string")
    .flatMap((candidate) => candidate.children.filter((child): child is string => typeof child === "string"))
    .join("");
}

function hasText(node: ReactTestInstance, value: string) {
  return node.children.some((child) => child === value);
}
