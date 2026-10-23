/**
 * Whether replacing the page now would drop input the user has not sent: a
 * draft, a selected attachment, or an upload in progress. Each owner of such
 * input reports a probe while it lives; a computer switch asks before it goes
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
