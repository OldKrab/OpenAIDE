import type {
  WorktreeId,
  WorktreeRepositoryId,
  WorktreeRepositorySnapshot,
  WorktreeSummary,
} from "@openaide/app-server-client";
import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ProjectOption } from "../../state/composerOptions";
import {
  WorktreesSettingsTab,
  type WorktreeSettingsIntents,
} from "./WorktreesSettingsTab";

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(() => vi.unstubAllGlobals());

describe("WorktreesSettingsTab", () => {
  it("labels the repository from its main worktree and hides managed storage paths", () => {
    const managedPath = "/home/dev/src/OpenAIDE/.openaide-web-dev/state/worktrees/repository-1/settings";
    const tree = renderCatalog([
      worktree({
        worktreeId: "main-root",
        name: "OpenAIDE",
        path: "/home/dev/src/OpenAIDE",
        isMain: true,
      }),
      worktree({
        worktreeId: "project-root",
        name: "Current project workspace",
        path: "/home/dev/src/OpenAIDE/.openaide-web-dev/state/worktrees/repository-1/project-root",
      }),
      worktree({
        worktreeId: "settings",
        name: "Settings refactor",
        path: managedPath,
      }),
    ]);
    expect(tree.root.findByProps({ className: "settings-worktree-project" })
      .findByType("header").findByType("strong").children).toEqual(["OpenAIDE"]);
    expect(renderedLocation(tree, "Settings refactor")).toBe("Managed by OpenAIDE");
    const rowNames = tree.root.findAllByProps({ className: "settings-worktree-row" })
      .map((row) => row.findAllByType("strong")[0].children.join(""));
    expect(rowNames).toEqual(["Settings refactor"]);
    expect(JSON.stringify(tree.toJSON())).not.toContain("Available");
  });

  it("shows status only when a worktree needs attention", () => {
    const tree = renderCatalog([
      worktree({
        availability: "unavailable",
        availabilityReason: "Folder is missing",
        worktreeId: "missing",
        name: "Missing worktree",
      }),
    ]);

    expect(JSON.stringify(tree.toJSON())).toContain("Unavailable");
  });

  it("abbreviates home and middle-elides long external paths", () => {
    const shortPath = "/home/dev/src/OpenAIDE-feature";
    const longPath = "/home/dev/src/client/projects/archive/OpenAIDE/feature-name";
    const tree = renderCatalog([
      worktree({
        worktreeId: "short-external",
        name: "Short external",
        ownership: "external",
        path: shortPath,
      }),
      worktree({
        worktreeId: "long-external",
        name: "Long external",
        ownership: "external",
        path: longPath,
      }),
    ]);

    expect(renderedLocation(tree, "Short external")).toBe("~/src/OpenAIDE-feature");
    expect(renderedLocation(tree, "Long external")).toBe("~/src/…/OpenAIDE/feature-name");
  });

  it("summarizes OpenAIDE storage paths even when another server owns them", () => {
    const generatedPath = "/home/dev/src/OpenAIDE/.openaide-web-dev/state/worktrees/repository-1/worktree-2";
    const tree = renderCatalog([
      worktree({
        worktreeId: "linked-generated",
        name: "Linked generated worktree",
        ownership: "external",
        path: generatedPath,
      }),
    ]);

    expect(renderedLocation(tree, "Linked generated worktree")).toBe("OpenAIDE storage");
  });

  it("copies the exact path from the catalog row without a native path tooltip", async () => {
    const path = "/home/dev/src/OpenAIDE/.worktrees/settings-refactor";
    const writeText = vi.fn();
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    const tree = renderCatalog([
      worktree({ name: "Settings refactor", path }),
    ]);

    await act(async () => {
      await tree.root.findByProps({ "aria-label": "Copy full path for Settings refactor" })
        .props.onClick();
    });

    expect(writeText).toHaveBeenCalledWith(path);
    expect(tree.root.findAllByProps({ title: path })).toHaveLength(0);
  });

  it("opens a worktree detail view from its row", () => {
    const tree = renderCatalog([
      worktree({
        worktreeId: "settings",
        name: "Settings refactor",
        path: "/home/dev/src/OpenAIDE/.worktrees/settings-refactor",
      }),
    ]);

    act(() => tree.root.findByProps({ "aria-label": "Open Settings refactor" }).props.onClick());

    expect(tree.root.findByProps({ className: "settings-worktree-detail" })).toBeTruthy();
    const back = tree.root.findByProps({ "aria-label": "Back to Worktrees" });
    expect(back.children).toContain(" Back to Worktrees");
    expect(JSON.stringify(tree.toJSON())).toContain("/home/dev/src/OpenAIDE/.worktrees/settings-refactor");
  });

  it("closes the create dialog with Escape", () => {
    const tree = renderCatalog([worktree({})]);
    const newWorktree = tree.root.findAllByType("button")
      .find((button) => button.children.includes(" New worktree"));

    act(() => newWorktree?.props.onClick());
    const dialog = tree.root.findByProps({ role: "dialog" });
    act(() => dialog.props.onKeyDown({ key: "Escape" }));

    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);
  });
});

function renderCatalog(worktrees: WorktreeSummary[]) {
  const projects: ProjectOption[] = [{
    label: "worktree-1bb8ff77-0062-4747-aa44-ec6de6cf7a25",
    projectId: "project-1",
    projectWorktreeId: "project-root",
    workspaceRoot: "/home/dev/src/OpenAIDE",
    worktreeRepositoryId: "repository-1",
  }];
  const repositories: Record<string, WorktreeRepositorySnapshot> = {
    "repository-1": {
      repositoryId: "repository-1" as WorktreeRepositoryId,
      revision: 1,
      worktrees,
    },
  };
  let tree: ReturnType<typeof create> | undefined;
  act(() => {
    tree = create(
      <WorktreesSettingsTab
        intents={intents()}
        onNewTask={() => undefined}
        projects={projects}
        repositories={repositories}
      />,
    );
  });
  return tree!;
}

function renderedLocation(
  tree: ReturnType<typeof create>,
  worktreeName: string,
) {
  const row = tree.root.findAllByProps({ className: "settings-worktree-row" })
    .find((candidate) => candidate.findAllByType("strong")
      .some((label) => label.children.includes(worktreeName)));
  return row?.findByProps({ className: "settings-worktree-location" })
    .findByType("span").children.join("");
}

function intents(): WorktreeSettingsIntents {
  return {
    createWorktree: vi.fn(),
    openTask: vi.fn(),
    recreateWorktree: vi.fn(),
    refreshWorktrees: vi.fn(),
    removalPreflight: vi.fn(),
    removeWorktree: vi.fn(),
  };
}

function worktree(
  overrides: Omit<Partial<WorktreeSummary>, "worktreeId"> & { worktreeId?: string },
): WorktreeSummary {
  return {
    availability: "available",
    forgotten: false,
    head: { kind: "branch", name: "shushakov/settings-refactor", commit: "53cbe91d" },
    isMain: false,
    linkedTaskCount: 1,
    name: "Settings refactor",
    ownership: "managed",
    path: "/home/dev/src/OpenAIDE/.worktrees/settings-refactor",
    runningTaskCount: 0,
    worktreeId: "settings" as WorktreeId,
    ...overrides,
  } as WorktreeSummary;
}
