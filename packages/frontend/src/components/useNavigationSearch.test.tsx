import { act, create } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TaskNavigationSearchResult } from "@openaide/app-server-client";
import { NAVIGATION_SEARCH_DEBOUNCE_MS, useNavigationSearch, type RunNavigationSearch } from "./useNavigationSearch";

type Probe = ReturnType<typeof useNavigationSearch>;

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  vi.useFakeTimers();
});

afterEach(() => vi.useRealTimers());

function result(...ids: string[]): TaskNavigationSearchResult {
  return { hits: ids.map((id) => ({ id })) as never, truncated: false };
}

function mount(initialText: string, run: RunNavigationSearch | undefined) {
  const latest: { current?: Probe } = {};
  function Harness({ text }: { text: string }) {
    latest.current = useNavigationSearch(text, run);
    return null;
  }
  let tree!: ReturnType<typeof create>;
  act(() => { tree = create(<Harness text={initialText} />); });
  return {
    latest,
    setText: (text: string) => act(() => tree.update(<Harness text={text} />)),
  };
}

const settle = () => act(async () => { await vi.advanceTimersByTimeAsync(NAVIGATION_SEARCH_DEBOUNCE_MS); });

describe("useNavigationSearch", () => {
  it("stays idle without text and sends nothing", async () => {
    const run = vi.fn(async () => result());
    const { latest } = mount("   ", run);

    await settle();

    expect(latest.current!.state).toEqual({ status: "idle" });
    expect(run).not.toHaveBeenCalled();
  });

  it("waits out typing and sends one request for the final trimmed text", async () => {
    const run = vi.fn(async () => result("a"));
    const { latest, setText } = mount("r", run);

    setText("re");
    setText(" rel ");
    expect(run).not.toHaveBeenCalled();
    await settle();

    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith("rel");
    expect(latest.current!.state.status).toBe("ready");
  });

  it("ignores a reply that arrives after newer text was sent", async () => {
    const replies: Array<(value: TaskNavigationSearchResult) => void> = [];
    const run = vi.fn(() => new Promise<TaskNavigationSearchResult>((resolve) => { replies.push(resolve); }));
    const { latest, setText } = mount("old", run);
    await settle();
    setText("new");
    await settle();

    await act(async () => { replies[1]!(result("new-hit")); });
    await act(async () => { replies[0]!(result("old-hit")); });

    expect(latest.current!.state).toEqual({ status: "ready", result: result("new-hit") });
  });

  it("ignores a reply that lands after the field was cleared", async () => {
    let reply!: (value: TaskNavigationSearchResult) => void;
    const run = vi.fn(() => new Promise<TaskNavigationSearchResult>((resolve) => { reply = resolve; }));
    const { latest, setText } = mount("text", run);
    await settle();

    setText("");
    await act(async () => { reply(result("late")); });

    expect(latest.current!.state).toEqual({ status: "idle" });
  });

  it("keeps the previous answer on screen while the next text loads", async () => {
    const run = vi.fn(async (text: string) => result(text));
    const { latest, setText } = mount("one", run);
    await settle();

    setText("one two");

    expect(latest.current!.state).toEqual({ status: "loading", previous: result("one") });
  });

  it("reports a failure and retries on request", async () => {
    const run = vi.fn<RunNavigationSearch>()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce(result("ok"));
    const { latest } = mount("text", run);
    await settle();
    expect(latest.current!.state).toEqual({ status: "error", message: "offline" });

    act(() => latest.current!.retry());
    await settle();

    expect(latest.current!.state).toEqual({ status: "ready", result: result("ok") });
  });
});
