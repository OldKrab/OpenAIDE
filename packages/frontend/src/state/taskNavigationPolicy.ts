/** Number of additional Task Navigation rows revealed by one user action. */
export function taskNavigationPageSize(projectCount: number): number {
  return projectCount >= 3 ? 7 : 10;
}

/**
 * Fixed per-Project history limit for Task Navigation. Live rows show on top of it, so it stays
 * small with many Projects; it depends only on the Project count, so collapsing or expanding one
 * Project never resizes another.
 */
export function initialTaskNavigationRowsPerProject(projectCount: number): number {
  if (projectCount <= 1) return 20;
  if (projectCount === 2) return 7;
  return 3;
}
