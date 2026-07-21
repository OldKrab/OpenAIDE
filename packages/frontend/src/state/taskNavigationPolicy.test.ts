import { describe, expect, it } from "vitest";
import { initialTaskNavigationRowsPerProject, TASK_NAVIGATION_PAGE_SIZE } from "./taskNavigationPolicy";

describe("Task Navigation presentation budget", () => {
  it("allocates 20 rows for one Project, 10 for two, and 7 for three or more", () => {
    expect(initialTaskNavigationRowsPerProject(1)).toBe(20);
    expect(initialTaskNavigationRowsPerProject(2)).toBe(10);
    expect(initialTaskNavigationRowsPerProject(5)).toBe(7);
  });

  it("reveals ten more rows per explicit load-more action", () => {
    expect(TASK_NAVIGATION_PAGE_SIZE).toBe(10);
  });
});
