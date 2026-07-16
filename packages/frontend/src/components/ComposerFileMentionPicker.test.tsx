import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskFileBrowserCallbacks } from "./appControllerCallbackTypes";
import {
  type FileMentionToken,
  useFileMentionPicker,
} from "./ComposerFileMentions";

describe("workspace file mention picker", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  });

  afterEach(() => {
    vi.useRealTimers();
    delete (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;
  });

  it("does not expose stale results after the caret leaves the @ token", async () => {
    vi.useFakeTimers();
    const browser: TaskFileBrowserCallbacks = {
      attachEmbedded: vi.fn<TaskFileBrowserCallbacks["attachEmbedded"]>(),
      attachFileReference: vi.fn<TaskFileBrowserCallbacks["attachFileReference"]>(),
      attachPastedImage: vi.fn<TaskFileBrowserCallbacks["attachPastedImage"]>(),
      listDirectory: vi.fn<TaskFileBrowserCallbacks["listDirectory"]>(),
      listRoots: vi.fn<TaskFileBrowserCallbacks["listRoots"]>(),
      ownerKey: "task:test",
      searchFiles: vi.fn(async () => ({
        paths: ["package.json"],
        state: "ready" as const,
        taskId: "task-1" as never,
      })),
    };
    const observed: Array<{ token?: FileMentionToken; visible: boolean }> = [];
    const token = { start: 0, end: 8, query: "package" };
    let renderer: ReturnType<typeof create>;

    await act(async () => {
      renderer = create(<PickerProbe browser={browser} observed={observed} token={token} />);
      await vi.advanceTimersByTimeAsync(40);
    });
    expect(observed.at(-1)?.visible).toBe(true);

    observed.length = 0;
    act(() => {
      renderer.update(<PickerProbe browser={browser} observed={observed} token={undefined} />);
    });

    expect(observed).not.toContainEqual({ token: undefined, visible: true });
  });
});

function PickerProbe({ browser, observed, token }: {
  browser: TaskFileBrowserCallbacks;
  observed: Array<{ token?: FileMentionToken; visible: boolean }>;
  token?: FileMentionToken;
}) {
  const [state] = useFileMentionPicker(browser, token);
  observed.push({ token, visible: Boolean(state) });
  return null;
}
