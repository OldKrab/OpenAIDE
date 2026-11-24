import path from "node:path";
import { fileURLToPath } from "node:url";

/**
 * Deletes main's Actions caches that a newer cache of the same family has replaced.
 *
 * The repository's 10 GB cache budget holds main's CI caches and the release builds'
 * caches with little room to spare. Each lockfile change saves a new set beside the
 * old one, and GitHub then evicts by last access: the release caches, touched only by
 * artifact runs, go before the replaced set that no run will restore again. Deleting
 * the replaced set keeps the release caches.
 */

/**
 * The family a cache key belongs to, or null for keys this script never touches.
 * A family is what a later run restores from, so only its newest cache is useful.
 */
export function cacheFamily(key) {
  // Swatinem/rust-cache: `<restore key>-<8-hex lockfile hash>`, where the restore key
  // is `v0-rust-[key-]<job>-<OS>-<arch>-<environment hash>`. Different environment
  // hashes stay separate families: runner images can alternate during a rollout.
  const rust = /^(v0-rust-.+)-[0-9a-f]{8}$/.exec(key);
  if (rust) return rust[1];
  // actions/setup-node: `node-cache-<OS>-<arch>-<manager>-<64-hex lockfile hash>`.
  const node = /^(node-cache-.+)-[0-9a-f]{64}$/.exec(key);
  if (node) return node[1];
  return null;
}

/** Every cache on `ref` whose family has a newer cache; other refs and keys are kept. */
export function supersededCaches(caches, ref = "refs/heads/main") {
  const newest = new Map();
  for (const cache of caches) {
    const family = cache.ref === ref ? cacheFamily(cache.key) : null;
    if (!family) continue;
    const kept = newest.get(family);
    if (!kept || isNewer(cache, kept)) newest.set(family, cache);
  }
  return caches.filter((cache) => {
    const family = cache.ref === ref ? cacheFamily(cache.key) : null;
    return family !== null && newest.get(family) !== cache;
  }).map((cache) => ({ ...cache, family: cacheFamily(cache.key) }));
}

function isNewer(cache, other) {
  const difference = Date.parse(cache.created_at) - Date.parse(other.created_at);
  return difference > 0 || (difference === 0 && cache.id > other.id);
}

/**
 * Lists `ref`'s caches, then deletes the superseded ones. A cache already gone (404)
 * was evicted meanwhile and counts as freed by GitHub, not as a failure.
 * Emits metadata-only `cache_prune` events through `onEvent`.
 */
export async function pruneSupersededCaches({
  repository,
  token,
  ref = "refs/heads/main",
  dryRun = false,
  fetchImpl = fetch,
  onEvent = () => {},
}) {
  if (!repository || !token) throw new Error("repository and token are required to prune caches");
  const started = performance.now();
  const summary = {
    listed: 0, pages: 0, superseded: 0, superseded_bytes: 0, deleted: 0, gone: 0, failed: 0, freed_bytes: 0,
  };
  const emit = (outcome, extra = {}) => onEvent({
    operation: "cache_prune", outcome, ref, dry_run: dryRun,
    duration_ms: Math.round(performance.now() - started), ...summary, ...extra,
  });
  emit("started");

  let caches;
  try {
    caches = await listCaches({ repository, token, ref, fetchImpl, summary });
  } catch (error) {
    emit("failed", { phase: "list", error_kind: error.kind ?? "network", status: error.status ?? null });
    throw error;
  }
  const superseded = supersededCaches(caches, ref);
  summary.superseded = superseded.length;
  summary.superseded_bytes = superseded.reduce((total, cache) => total + cache.size_in_bytes, 0);

  for (const cache of superseded) {
    const fields = { cache_id: cache.id, family: cache.family, size_bytes: cache.size_in_bytes, created_at: cache.created_at };
    if (dryRun) {
      onEvent({ operation: "cache_prune_delete", outcome: "skipped_dry_run", ...fields });
      continue;
    }
    const deleteStarted = performance.now();
    const outcome = await deleteCache({ repository, token, id: cache.id, fetchImpl });
    if (outcome.result === "deleted") {
      summary.deleted += 1;
      summary.freed_bytes += cache.size_in_bytes;
    } else if (outcome.result === "gone") {
      summary.gone += 1;
    } else {
      summary.failed += 1;
    }
    onEvent({
      operation: "cache_prune_delete", outcome: outcome.result, ...fields,
      status: outcome.status, error_kind: outcome.errorKind ?? null,
      duration_ms: Math.round(performance.now() - deleteStarted),
    });
  }
  emit(summary.failed > 0 ? "failed" : "completed", summary.failed > 0 ? { phase: "delete" } : {});
  return summary;
}

// Bounds pagination against an API that kept answering; 50 pages is 5,000 caches.
const MAX_PAGES = 50;

async function listCaches({ repository, token, ref, fetchImpl, summary }) {
  const caches = [];
  for (let page = 1; page <= MAX_PAGES; page += 1) {
    const query = new URLSearchParams({ ref, per_page: "100", page: String(page), sort: "created_at", direction: "desc" });
    const response = await fetchImpl(`https://api.github.com/repos/${repository}/actions/caches?${query}`, {
      headers: headers(token),
    });
    if (!response.ok) {
      throw Object.assign(new Error(`GitHub cache list failed (${response.status})`), {
        kind: "http", status: response.status,
      });
    }
    const body = await response.json();
    const items = body.actions_caches ?? [];
    summary.pages = page;
    caches.push(...items);
    summary.listed = caches.length;
    if (items.length === 0 || caches.length >= (body.total_count ?? 0)) return caches;
  }
  throw Object.assign(new Error(`GitHub cache list exceeded ${MAX_PAGES} pages`), { kind: "too_many_pages" });
}

async function deleteCache({ repository, token, id, fetchImpl }) {
  try {
    const response = await fetchImpl(`https://api.github.com/repos/${repository}/actions/caches/${id}`, {
      method: "DELETE",
      headers: headers(token),
    });
    if (response.status === 204) return { result: "deleted", status: 204 };
    if (response.status === 404) return { result: "gone", status: 404 };
    return { result: "failed", status: response.status, errorKind: "http" };
  } catch (error) {
    return { result: "failed", status: null, errorKind: error.name ?? "network" };
  }
}

function headers(token) {
  return {
    accept: "application/vnd.github+json",
    authorization: `Bearer ${token}`,
    "x-github-api-version": "2022-11-28",
  };
}

async function main() {
  const summary = await pruneSupersededCaches({
    repository: process.env.GITHUB_REPOSITORY,
    token: process.env.GITHUB_TOKEN,
    dryRun: process.argv.includes("--dry-run"),
    onEvent: (event) => console.log(JSON.stringify(event)),
  });
  if (summary.failed > 0) process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
