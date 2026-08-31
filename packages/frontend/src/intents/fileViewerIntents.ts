import {
  FILE_VIEWER_OPEN,
  FILE_VIEWER_OPEN_FROM_HANDLE,
  FILE_VIEWER_REFRESH,
  FILE_VIEWER_RELEASE,
  type BackendConnection,
  type FileViewerHandleId,
  type FileViewerSnapshot,
  type TaskId,
} from "@openaide/app-server-client";

type FileViewerConnection = Pick<BackendConnection, "request">;

export async function openFileViewer(
  connection: FileViewerConnection,
  taskId: string,
  path: string,
  line?: number,
): Promise<FileViewerSnapshot> {
  return requestFileViewerSnapshot(connection, "open", FILE_VIEWER_OPEN, {
    taskId: taskId as TaskId,
    path,
    ...(line ? { line } : {}),
  }, { task_id: taskId });
}

export async function openFileViewerFromHandle(
  connection: FileViewerConnection,
  handle: string,
  href: string,
): Promise<FileViewerSnapshot> {
  return requestFileViewerSnapshot(connection, "open_from_handle", FILE_VIEWER_OPEN_FROM_HANDLE, {
    handle: handle as FileViewerHandleId,
    href,
  }, { handle_kind: "opaque" });
}

export async function refreshFileViewer(
  connection: FileViewerConnection,
  handle: string,
  line?: number,
): Promise<FileViewerSnapshot> {
  return requestFileViewerSnapshot(connection, "refresh", FILE_VIEWER_REFRESH, {
    handle: handle as FileViewerHandleId,
    ...(line ? { line } : {}),
  }, { handle_kind: "opaque" });
}

export async function releaseFileViewer(connection: FileViewerConnection, handle: string) {
  const operationId = crypto.randomUUID();
  const startedAt = performance.now();
  console.info(`file_viewer_release_started operation_id=${operationId}`);
  try {
    await connection.request(FILE_VIEWER_RELEASE, { handle: handle as FileViewerHandleId });
    console.info(
      `file_viewer_release_completed operation_id=${operationId} outcome=success duration_ms=${Math.round(performance.now() - startedAt)}`,
    );
  } catch (error) {
    console.warn(
      `file_viewer_release_completed operation_id=${operationId} outcome=failure error_kind=${errorKind(error)} duration_ms=${Math.round(performance.now() - startedAt)}`,
    );
    throw error;
  }
}

async function requestFileViewerSnapshot(
  connection: FileViewerConnection,
  operation: string,
  method: typeof FILE_VIEWER_OPEN | typeof FILE_VIEWER_OPEN_FROM_HANDLE | typeof FILE_VIEWER_REFRESH,
  params: object,
  fields: Record<string, string>,
): Promise<FileViewerSnapshot> {
  const operationId = crypto.randomUUID();
  const startedAt = performance.now();
  const extra = Object.entries(fields).map(([key, value]) => `${key}=${value}`).join(" ");
  console.info(`file_viewer_${operation}_started operation_id=${operationId} ${extra}`);
  try {
    const snapshot = await connection.request(method, params as never);
    console.info(
      `file_viewer_${operation}_completed operation_id=${operationId} ${extra} outcome=${snapshot.kind} duration_ms=${Math.round(performance.now() - startedAt)}`,
    );
    return snapshot;
  } catch (error) {
    console.warn(
      `file_viewer_${operation}_completed operation_id=${operationId} ${extra} outcome=failure error_kind=${errorKind(error)} duration_ms=${Math.round(performance.now() - startedAt)}`,
    );
    throw error;
  }
}

function errorKind(error: unknown) {
  return error instanceof Error && error.name ? error.name : typeof error;
}
