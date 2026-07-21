/** Number of additional Task Navigation rows revealed by one user action. */
export const TASK_NAVIGATION_PAGE_SIZE = 10;

/** Keeps one Project useful without letting many Projects monopolize the sidebar. */
export function initialTaskNavigationRowsPerProject(projectCount: number): number {
  if (projectCount <= 1) return 20;
  if (projectCount === 2) return 10;
  return 7;
}
