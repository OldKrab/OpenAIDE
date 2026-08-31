/** Number of additional Task Navigation rows revealed by one user action. */
export function taskNavigationPageSize(projectCount: number): number {
  return projectCount >= 3 ? 7 : 10;
}

/** Keeps one Project useful without letting many Projects monopolize the sidebar. */
export function initialTaskNavigationRowsPerProject(projectCount: number): number {
  if (projectCount <= 1) return 20;
  if (projectCount === 2) return 7;
  return 5;
}
