import { describe, expect, it, vi } from "vitest";
import type { FrontendShell } from "../services/frontendShell";
import type { WebviewBootstrap } from "../state/surfaceTypes";
import { projectFilesRoute } from "./projectFilesRoute";

const shellBootstrap = { kind: "web", navigationMode: "project" } as const;
const filesBootstrap: WebviewBootstrap = { surface: "task", shell: shellBootstrap, projectId: "p1", projectFiles: true };

function shell(navigation: Partial<FrontendShell["navigation"]>, fileViewer = true) {
  return { fileViewer, navigation } as unknown as FrontendShell;
}

describe("projectFilesRoute", () => {
  it("opens and closes the page through a shell that routes it", () => {
    const openProjectFiles = vi.fn();
    const closeProjectFiles = vi.fn();
    const route = projectFilesRoute(filesBootstrap, shell({ openProjectFiles, closeProjectFiles }));

    route.browse?.("p2");
    route.page?.onClose();

    expect(route.page?.projectId).toBe("p1");
    expect(openProjectFiles).toHaveBeenCalledWith("p2");
    expect(closeProjectFiles).toHaveBeenCalledWith("p1");
  });

  it("shows no page on any other route", () => {
    const navigation = { openProjectFiles: vi.fn(), closeProjectFiles: vi.fn() };

    expect(projectFilesRoute({ ...filesBootstrap, projectFiles: undefined }, shell(navigation)).page).toBeUndefined();
    expect(projectFilesRoute({ ...filesBootstrap, taskId: "t1" }, shell(navigation)).page).toBeUndefined();
    expect(projectFilesRoute({ surface: "invalid" }, shell(navigation)).page).toBeUndefined();
  });

  it("offers nothing in a shell without a file viewer or without the route", () => {
    const navigation = { openProjectFiles: vi.fn(), closeProjectFiles: vi.fn() };

    expect(projectFilesRoute(filesBootstrap, shell(navigation, false))).toEqual({ browse: undefined, page: undefined });
    expect(projectFilesRoute(filesBootstrap, shell({}))).toEqual({ browse: undefined, page: undefined });
  });
});
