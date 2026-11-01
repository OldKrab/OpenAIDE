import { createContext, useContext, useEffect, useSyncExternalStore } from "react";
import type { BackendConnection, TaskId } from "@openaide/app-server-client";
import { readProjectFiles } from "../intents/projectFileIntents";

/** The App Server answers at most this many paths per request. */
const BATCH_SIZE = 100;
/** A path found missing is asked again when its message is shown after this long. */
const MISSING_RECHECK_MS = 15_000;
/** After a failed check, paths stay unlinked this long instead of asking again. */
const FAILURE_BACKOFF_MS = 60_000;
/** Chat is virtualized, so its rows mount over several commits; their paths wait this long to share a request. */
const COALESCE_MS = 150;

export type FileReferenceState = "file" | "missing" | "pending" | "unchecked";
type CheckPaths = (paths: string[]) => Promise<string[]>;
type ScheduleDrain = (drain: () => void) => void;

/**
 * Remembers, for one Task on one connection, which workspace-relative paths named by the Agent are
 * files. Paths requested close together go to the App Server in one request, only one request runs
 * at a time, and a backlog waits between requests; a file stays a file, and a missing path is checked
 * again later, since the Agent may create it after mentioning it.
 */
export class ExistingFileReferences {
  private readonly known = new Map<string, { file: boolean; checkedAt: number }>();
  private readonly queued = new Set<string>();
  private readonly inFlight = new Set<string>();
  private readonly listeners = new Set<() => void>();
  private draining = false;
  private failedUntil = 0;

  constructor(
    private readonly check: CheckPaths,
    private readonly now: () => number = Date.now,
    private readonly schedule: ScheduleDrain = (drain) => { setTimeout(drain, COALESCE_MS); },
  ) {}

  readonly subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  state(path: string): FileReferenceState {
    const known = this.known.get(path);
    if (known) return known.file ? "file" : "missing";
    return this.now() < this.failedUntil ? "missing" : "pending";
  }

  /** Asks about a path unless its answer is still current. */
  request(path: string) {
    const known = this.known.get(path);
    if (known?.file || this.queued.has(path) || this.inFlight.has(path)) return;
    if (known && this.now() - known.checkedAt < MISSING_RECHECK_MS) return;
    if (this.now() < this.failedUntil) return;
    this.queued.add(path);
    if (this.draining) return;
    this.draining = true;
    this.schedule(() => void this.drain());
  }

  /** Sends one request; what is queued after it waits a window, so it shares the next one. */
  private async drain() {
    const batch = [...this.queued].slice(0, BATCH_SIZE);
    batch.forEach((path) => {
      this.queued.delete(path);
      this.inFlight.add(path);
    });
    let files: Set<string> | undefined;
    try {
      files = new Set(await this.check(batch));
    } catch {
      // The intent has logged the failure; an older App Server lacks the method entirely.
      this.failedUntil = this.now() + FAILURE_BACKOFF_MS;
      this.queued.clear();
    }
    const checkedAt = this.now();
    for (const path of batch) {
      this.inFlight.delete(path);
      if (files) this.known.set(path, { file: files.has(path), checkedAt });
    }
    this.listeners.forEach((listener) => listener());
    if (this.queued.size > 0) this.schedule(() => void this.drain());
    else this.draining = false;
  }
}

type FileCheckConnection = Pick<BackendConnection, "request">;

/** Outlives the views of a Task, which mount again on every return to it, and goes with the connection. */
const referencesByConnection = new WeakMap<FileCheckConnection, Map<string, ExistingFileReferences>>();

/** The answers for one Task's workspace, shared by every view of it on this connection. */
export function existingFileReferencesFor(connection: FileCheckConnection, taskId: string) {
  let references = referencesByConnection.get(connection);
  if (!references) referencesByConnection.set(connection, references = new Map());
  let taskReferences = references.get(taskId);
  if (!taskReferences) references.set(taskId, taskReferences = checkingProjectFiles(connection, taskId));
  return taskReferences;
}

/** Checks paths through the read-only Project Files intent. */
function checkingProjectFiles(connection: FileCheckConnection, taskId: string) {
  return new ExistingFileReferences(async (paths) => {
    const result = await readProjectFiles(connection, "existing", {
      taskId: taskId as TaskId,
      path: "",
      query: "",
      content: false,
      caseSensitive: false,
      includeIgnored: false,
      cursor: 0,
      paths,
    });
    if (result.error) throw new Error(result.error);
    return result.entries.map((entry) => entry.path);
  });
}

export const ExistingFileReferencesContext = createContext<ExistingFileReferences | undefined>(undefined);

const subscribeToNothing = () => () => {};

/** Whether [path] names a file; "unchecked" when no Task workspace can answer. */
export function useFileReferenceState(path: string | undefined): FileReferenceState {
  const references = useContext(ExistingFileReferencesContext);
  const snapshot = () => (references && path ? references.state(path) : "unchecked");
  const state = useSyncExternalStore(references?.subscribe ?? subscribeToNothing, snapshot, snapshot);
  useEffect(() => {
    if (references && path) references.request(path);
  }, [references, path]);
  return state;
}
