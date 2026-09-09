import { act, create } from "react-test-renderer";
import { expect, it, vi } from "vitest";
import { FILE_VIEWER_RELEASE, type FileViewerSnapshot } from "@openaide/app-server-client";
import { useTaskFileViewer } from "./useTaskFileViewer";
it("discards and releases an open completed after switching Tasks", async () => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  let finish!: (value: FileViewerSnapshot) => void;
  const request = vi.fn((method: string) =>
    method === FILE_VIEWER_RELEASE
      ? Promise.resolve({})
      : new Promise<FileViewerSnapshot>((resolve) => {
          finish = resolve;
        }),
  );
  let viewer!: ReturnType<typeof useTaskFileViewer>;
  function Harness({ taskId }: { taskId: string }) {
    viewer = useTaskFileViewer({ taskId, enabled: true, connection: { request: request as never } });
    return <span>{viewer.tabs.length}</span>;
  }
  let tree!: ReturnType<typeof create>;
  act(() => {
    tree = create(<Harness taskId="one" />);
  });
  let opening!: Promise<void>;
  act(() => {
    opening = viewer.openPath("src/a.ts");
  });
  act(() => {
    tree.update(<Harness taskId="two" />);
  });
  await act(async () => {
    finish({
      handle: "h" as FileViewerSnapshot["handle"],
      displayPath: "src/a.ts",
      basename: "a.ts",
      kind: "source",
      text: "old task content",
      truncated: false,
    });
    await opening;
  });
  expect(viewer.tabs).toHaveLength(0);
  expect(request).toHaveBeenCalledWith(FILE_VIEWER_RELEASE, { handle: "h" });
  act(() => tree.unmount());
});
