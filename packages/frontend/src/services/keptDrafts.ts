/** The text of unsent drafts, which can outlive the page across a computer switch. */
export type DraftTexts = { newTask?: string; tasks: Record<string, string> };

const KEY_PREFIX = "openaide.keptDrafts:";
/** A computer the user never returns to must not hold their text forever. */
const RETENTION_MS = 30 * 24 * 60 * 60 * 1000;

/**
 * Keeps one computer's draft text on this device until the page next opens there
 * (ADR-0070). Text only: attachments belong to the page and their App Server. Storage
 * is per origin and origins can be shared, so every entry names its computer.
 */
export function keepDrafts(computer: string, drafts: DraftTexts, storage = globalStorage()): boolean {
  try {
    if (draftCount(drafts) === 0) {
      storage?.removeItem(KEY_PREFIX + computer);
      return true;
    }
    storage!.setItem(KEY_PREFIX + computer, JSON.stringify({ ...drafts, keptAt: Date.now() }));
    return true;
  } catch {
    return false;
  }
}

export function dropKeptDrafts(computer: string, storage = globalStorage()) {
  try {
    storage?.removeItem(KEY_PREFIX + computer);
  } catch {
    // Blocked storage holds nothing to drop.
  }
}

/** Hands one computer's drafts to the page once, and ends any entry past its retention. */
export function takeKeptDrafts(computer: string, storage = globalStorage(), now = Date.now()): DraftTexts | undefined {
  try {
    const stored = storage?.getItem(KEY_PREFIX + computer);
    dropExpired(storage, now);
    if (!stored) return undefined;
    storage?.removeItem(KEY_PREFIX + computer);
    const value = JSON.parse(stored) as Partial<DraftTexts> & { keptAt?: unknown };
    if (typeof value.keptAt !== "number" || now - value.keptAt > RETENTION_MS) return undefined;
    const tasks = Object.entries(value.tasks ?? {}).filter(([, text]) => typeof text === "string");
    return {
      ...(typeof value.newTask === "string" ? { newTask: value.newTask } : {}),
      tasks: Object.fromEntries(tasks),
    };
  } catch {
    // Unreadable drafts cannot be restored; the page opens with empty Composers.
    console.warn("computer_switch_drafts_restore_failed error_kind=storage");
    return undefined;
  }
}

export function draftCount(drafts: DraftTexts) {
  return Object.keys(drafts.tasks).length + (drafts.newTask === undefined ? 0 : 1);
}

function dropExpired(storage: Storage | undefined, now: number) {
  for (let index = (storage?.length ?? 0) - 1; index >= 0; index--) {
    const key = storage?.key(index);
    if (!key?.startsWith(KEY_PREFIX)) continue;
    let keptAt: unknown;
    try {
      keptAt = (JSON.parse(storage?.getItem(key) ?? "null") as { keptAt?: unknown } | null)?.keptAt;
    } catch {
      keptAt = undefined;
    }
    if (typeof keptAt !== "number" || now - keptAt > RETENTION_MS) storage?.removeItem(key);
  }
}

function globalStorage(): Storage | undefined {
  try {
    return typeof localStorage === "undefined" ? undefined : localStorage;
  } catch {
    return undefined;
  }
}
