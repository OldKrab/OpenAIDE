import {
  FILE_VIEWER_LIST_DIRECTORY,
  FILE_VIEWER_SEARCH,
  FILE_VIEWER_CHANGES,
  FILE_VIEWER_DIFF,
  type BackendConnection,
  type ClientRequestId,
  type ProjectFilesParams,
  type ProjectFilesResult,
} from "@openaide/app-server-client";
export type ProjectFileOperation = "files" | "search" | "changes" | "diff";
const methods = {
  files: FILE_VIEWER_LIST_DIRECTORY,
  search: FILE_VIEWER_SEARCH,
  changes: FILE_VIEWER_CHANGES,
  diff: FILE_VIEWER_DIFF,
} as const;
/** Read-only Task Workspace intent. Backend owns root authorization, Git and result bounds.
 * Callers discard superseded results; the server bounds each scan and subprocess lifetime. */
export async function readProjectFiles(
  connection: Pick<BackendConnection, "request">,
  operation: ProjectFileOperation,
  params: ProjectFilesParams,
): Promise<ProjectFilesResult> {
  const id = crypto.randomUUID();
  const start = performance.now();
  console.info(`project_files_started operation=${operation} operation_id=${id} attempt=1`);
  try {
    const result = await connection.request(methods[operation], params, { clientRequestId: id as ClientRequestId });
    console.info(
      `project_files_completed operation=${operation} operation_id=${id} attempt=1 outcome=${result.error ? "failure" : "success"} error_kind=${result.error ?? "none"} truncated=${result.truncated} duration_ms=${Math.round(performance.now() - start)}`,
    );
    return result;
  } catch {
    console.warn(
      `project_files_completed operation=${operation} operation_id=${id} attempt=1 outcome=transport_failure duration_ms=${Math.round(performance.now() - start)}`,
    );
    throw new Error("Project files are unavailable. Try again when connected.");
  }
}
