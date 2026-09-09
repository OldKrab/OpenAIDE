import { mkdtemp, mkdir, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { validatedWorkspacePath } from "./workspaceBoundary";

const workspace = vi.hoisted(() => ({ workspaceFolders: [] as { uri: { fsPath: string } }[] }));
vi.mock("vscode", () => ({ workspace }));

const fixtureRoots: string[] = [];
afterEach(async () => {
  await Promise.all(fixtureRoots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
  workspace.workspaceFolders = [];
});

describe("workspace filesystem authorization", () => {
  it("rejects a dangling file symlink that would create a file outside the workspace", async () => {
    const root = await mkdtemp(path.join(tmpdir(), "openaide-workspace-boundary-"));
    fixtureRoots.push(root);
    const project = path.join(root, "project");
    await mkdir(project);
    workspace.workspaceFolders = [{ uri: { fsPath: project } }];
    const link = path.join(project, "new-file.txt");
    await symlink(path.join(root, "outside.txt"), link);

    await expect(validatedWorkspacePath(link, "write-target")).rejects.toThrow();
  });

  it("allows files whose names begin with two dots inside the workspace", async () => {
    const root = await mkdtemp(path.join(tmpdir(), "openaide-workspace-boundary-"));
    fixtureRoots.push(root);
    workspace.workspaceFolders = [{ uri: { fsPath: root } }];
    const file = path.join(root, "..notes");
    await writeFile(file, "project notes");

    await expect(validatedWorkspacePath(file, "existing")).resolves.toBe(file);
  });
});
