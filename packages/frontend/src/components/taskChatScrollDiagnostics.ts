export type ChatScrollGeometry = {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
  distanceFromBottom: number;
};

export type TaskChatScrollDiagnosticContext = {
  chatVersion: number;
  historySyncState: string;
  itemCount: number;
  itemKindCounts: Record<string, number>;
  olderItemCount: number;
  pendingPermissions: string[];
  snapshotRevision: number;
  taskStatus: string;
};

type ChatScrollDiagnosticEvent = {
  at: number;
  type: "geometry" | "intent" | "render" | "lifecycle" | "ownership" | "anomaly";
  geometry?: ChatScrollGeometry;
  intent?: "towardEarlier" | "towardLatest";
  context?: TaskChatScrollDiagnosticContext;
  state?: "mounted" | "unmounted";
  ownership?: "following" | "reading";
  reason?: string;
};

type ChatScrollDiagnosticTrace = {
  version: 1;
  taskId: string;
  frozen: boolean;
  events: ChatScrollDiagnosticEvent[];
};

const MAX_EVENTS = 200;
const USER_INTENT_WINDOW_MS = 500;

/** Captures redacted, tab-scoped evidence for unexpected Chat viewport jumps. */
export class TaskChatScrollDiagnostics {
  readonly #storageKey: string;
  #trace: ChatScrollDiagnosticTrace;
  #lastGeometry: ChatScrollGeometry | undefined;
  #lastContextFingerprint: string | undefined;
  #upwardIntentUntil = 0;

  constructor(taskId: string) {
    this.#storageKey = `openaide:scroll-diagnostics:${taskId}`;
    this.#trace = readTrace(this.#storageKey) ?? { version: 1, taskId, frozen: false, events: [] };
  }

  recordGeometry(geometry: ChatScrollGeometry) {
    if (this.#trace.frozen) return;
    const previous = this.#lastGeometry;
    this.#lastGeometry = geometry;
    if (
      previous
      && previous.distanceFromBottom <= 2
      && geometry.distanceFromBottom > geometry.clientHeight
      && Date.now() > this.#upwardIntentUntil
    ) {
      this.#append({ at: Date.now(), type: "anomaly", geometry });
      this.#trace.frozen = true;
      this.#persist();
      return;
    }
    this.#append({ at: Date.now(), type: "geometry", geometry });
  }

  recordIntent(intent: "towardEarlier" | "towardLatest") {
    if (this.#trace.frozen) return;
    if (intent === "towardEarlier") this.#upwardIntentUntil = Date.now() + USER_INTENT_WINDOW_MS;
    this.#append({ at: Date.now(), type: "intent", intent });
  }

  recordRender(context: TaskChatScrollDiagnosticContext) {
    if (this.#trace.frozen) return;
    const fingerprint = JSON.stringify(context);
    if (fingerprint === this.#lastContextFingerprint) return;
    this.#lastContextFingerprint = fingerprint;
    this.#append({ at: Date.now(), type: "render", context });
  }

  recordLifecycle(state: "mounted" | "unmounted") {
    if (this.#trace.frozen) return;
    this.#append({ at: Date.now(), type: "lifecycle", state });
  }

  recordOwnership(ownership: "following" | "reading", reason: string) {
    if (this.#trace.frozen) return;
    this.#append({ at: Date.now(), type: "ownership", ownership, reason });
  }

  #append(event: ChatScrollDiagnosticEvent) {
    this.#trace.events = [...this.#trace.events, event].slice(-MAX_EVENTS);
    this.#persist();
  }

  #persist() {
    try {
      globalThis.sessionStorage?.setItem(this.#storageKey, JSON.stringify(this.#trace));
    } catch {
      // Diagnostics must never interfere with Chat when browser storage is unavailable.
    }
  }
}

export function chatScrollGeometry(messageList: Pick<HTMLElement, "clientHeight" | "scrollHeight" | "scrollTop">) {
  return {
    scrollTop: messageList.scrollTop,
    scrollHeight: messageList.scrollHeight,
    clientHeight: messageList.clientHeight,
    distanceFromBottom: messageList.scrollHeight - messageList.scrollTop - messageList.clientHeight,
  };
}

function readTrace(storageKey: string): ChatScrollDiagnosticTrace | undefined {
  try {
    const raw = globalThis.sessionStorage?.getItem(storageKey);
    if (!raw) return undefined;
    const parsed = JSON.parse(raw) as Partial<ChatScrollDiagnosticTrace>;
    if (parsed.version !== 1 || !Array.isArray(parsed.events)) return undefined;
    return parsed as ChatScrollDiagnosticTrace;
  } catch {
    return undefined;
  }
}
