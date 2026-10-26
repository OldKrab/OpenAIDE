import { afterEach, describe, expect, it, vi } from "vitest";
import { WORKSPACE_LIST_ROOTS } from "@openaide/app-server-client";
import { AsyncOperationOwner } from "../state/asyncOperationOwner";
import { createInitialState } from "../state/store";
import { createNewTaskBrowserCallbacks } from "./newTaskBrowserCallbacks";

describe("workspace folder browser", () => {
  afterEach(() => vi.restoreAllMocks());

  function browser() {
    const asyncOperations = new AsyncOperationOwner();
    let answer: (value: unknown) => void = () => undefined;
    const request = vi.fn(() => new Promise((resolve) => { answer = resolve; }));
    const callbacks = createNewTaskBrowserCallbacks({
      asyncOperations,
      backendConnection: { request },
      dispatch: vi.fn(),
      newTaskController: {},
      state: createInitialState(),
    } as unknown as Parameters<typeof createNewTaskBrowserCallbacks>[0]);
    return { asyncOperations, answer: (value: unknown) => answer(value), browser: callbacks.workspaceBrowser!, request };
  }

  it("keeps a folder listing that finishes after a navigation", async () => {
    const { asyncOperations, answer, browser: folders, request } = browser();
    const roots = folders.listRoots();
    asyncOperations.beginNavigation("new-task");
    answer({ roots: [{ path: "/" }] });

    await expect(roots).resolves.toEqual([{ path: "/" }]);
    expect(request).toHaveBeenCalledWith(WORKSPACE_LIST_ROOTS, {});
  });

  it("drops a folder listing from a replaced App Server replica", async () => {
    vi.spyOn(console, "info").mockImplementation(() => undefined);
    const { asyncOperations, answer, browser: folders } = browser();
    const roots = folders.listRoots();
    asyncOperations.replaceReplica();
    answer({ roots: [{ path: "/" }] });

    await expect(roots).rejects.toThrow("Try again");
    expect(console.info).toHaveBeenCalledWith("workspace_browser_result_dropped method=workspace/listRoots reason=replica_replaced");
  });
});
