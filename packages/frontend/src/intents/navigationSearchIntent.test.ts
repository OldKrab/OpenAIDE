import { describe, expect, it, vi } from "vitest";
import { AppServerProtocolError, TASK_NAVIGATION_SEARCH, type BackendConnection } from "@openaide/app-server-client";
import { requestNavigationSearch, type NavigationSearchLog } from "./navigationSearchIntent";

function clock(...readings: number[]) {
  // timing: mocked — each read returns the next fixed reading.
  return () => readings.shift() ?? 0;
}

describe("requestNavigationSearch", () => {
  it("sends the text and logs only its length, the hit count and the duration", async () => {
    const request = vi.fn(async () => ({ hits: [{}, {}], truncated: true }));
    const log = vi.fn<NavigationSearchLog>();

    const result = await requestNavigationSearch(
      { request: request as unknown as BackendConnection["request"] },
      "secret phrase",
      log,
      ["project_1"],
      clock(10, 25),
    );

    expect(result.hits).toHaveLength(2);
    expect(request).toHaveBeenCalledWith(TASK_NAVIGATION_SEARCH, { text: "secret phrase", projectIds: ["project_1"] });
    expect(log.mock.calls).toEqual([
      ["navigation_search_started", { text_length: 13, project_filter_count: 1 }],
      ["navigation_search_completed", {
        text_length: 13,
        project_filter_count: 1,
        duration_ms: 15,
        hit_count: 2,
        truncated: true,
      }],
    ]);
    expect(JSON.stringify(log.mock.calls)).not.toContain("secret");
  });

  it("omits the Project filter when none is given", async () => {
    const request = vi.fn(async () => ({ hits: [], truncated: false }));

    await requestNavigationSearch(
      { request: request as unknown as BackendConnection["request"] },
      "a",
      vi.fn(),
    );

    expect(request).toHaveBeenCalledWith(TASK_NAVIGATION_SEARCH, { text: "a" });
  });

  it("classifies a protocol failure without its message and rethrows it", async () => {
    const error = new AppServerProtocolError({
      error: { code: "invalidRequest", message: "text 'secret' too long" },
    } as ConstructorParameters<typeof AppServerProtocolError>[0]);
    const request = vi.fn(async () => { throw error; });
    const log = vi.fn<NavigationSearchLog>();

    await expect(requestNavigationSearch(
      { request: request as unknown as BackendConnection["request"] },
      "secret",
      log,
      undefined,
      clock(0, 4),
    )).rejects.toBe(error);

    expect(log).toHaveBeenLastCalledWith("navigation_search_failed", {
      text_length: 6,
      project_filter_count: undefined,
      duration_ms: 4,
      error_name: error.name,
      error_code: "invalidRequest",
    });
  });
});
