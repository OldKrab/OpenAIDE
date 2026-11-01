import type { DraftTexts } from "./keptDrafts";

/**
 * Whether replacing the page now would drop input that cannot be kept: a selected
 * attachment, an upload in progress, or a send without an answer. Each owner of
 * such input reports a probe while it lives; a computer switch asks before it goes
 * (ADR-0070). Probes are read on demand, so idle owners cost nothing.
 */
const probes = new Set<() => boolean>();

export function reportUnsentInput(probe: () => boolean): () => void {
  probes.add(probe);
  return () => { probes.delete(probe); };
}

export function hasUnsentInput(): boolean {
  for (const probe of probes) {
    if (probe()) return true;
  }
  return false;
}

/** The page's draft text, read when a switch keeps it. One controller owns it per page. */
let readDrafts: (() => DraftTexts) | undefined;

export function reportDraftTexts(read: () => DraftTexts): () => void {
  readDrafts = read;
  return () => { if (readDrafts === read) readDrafts = undefined; };
}

export function unsentDraftTexts(): DraftTexts {
  return readDrafts?.() ?? { tasks: {} };
}
