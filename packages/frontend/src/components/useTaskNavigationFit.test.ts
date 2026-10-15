import { describe, expect, it } from "vitest";
import { allocateHistoryRows, type TaskNavigationFitGroup } from "./useTaskNavigationFit";

function group(key: string, history: number, overrides: Partial<TaskNavigationFitGroup> = {}): TaskNavigationFitGroup {
  return { key, history, cap: 5, moreAlways: false, ...overrides };
}

describe("allocateHistoryRows", () => {
  it("shares limited space one row at a time so no Project takes it all", () => {
    // Room for 6 rows after three Load More buttons: two history rows each.
    const shown = allocateHistoryRows([group("a", 9), group("b", 9), group("c", 9)], 9 * 32, 32, 32);
    expect([...shown.values()]).toEqual([2, 2, 2]);
  });

  it("gives unused space to Projects that still have history, up to the cap", () => {
    const shown = allocateHistoryRows([group("a", 1), group("b", 9), group("c", 0)], 40 * 32, 32, 32);
    expect(Object.fromEntries(shown)).toEqual({ a: 1, b: 5, c: 0 });
  });

  it("refunds Load More when the last history row fits", () => {
    // Two rows plus Load More need 96px; showing both rows removes the button and needs 64px.
    expect(allocateHistoryRows([group("a", 2)], 64, 32, 32).get("a")).toBe(2);
    expect(allocateHistoryRows([group("a", 2, { moreAlways: true })], 64, 32, 32).get("a")).toBe(1);
  });

  it("shows no history when live rows already fill the viewport", () => {
    expect(allocateHistoryRows([group("a", 4)], -10, 32, 32).get("a")).toBe(0);
  });
});
