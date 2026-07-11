#!/usr/bin/env node
import { mkdir, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const INDEX_URL = "https://agentclientprotocol.com/llms.txt";
const OPENAPI_URL = "https://agentclientprotocol.com/api-reference/openapi.json";
const CACHE_DIR = join(dirname(fileURLToPath(import.meta.url)), "..", ".cache");
const CACHE_MAX_AGE_MS = 1000 * 60 * 60 * 24 * 7;
const CACHE_MAX_FILES = 64;
const CACHE_READ_TTL_MS = 1000 * 60 * 60 * 12;

const command = process.argv[2] ?? "help";
const query = process.argv.slice(3).join(" ").trim();

async function main() {
  if (command === "help" || command === "--help" || command === "-h") {
    printHelp();
    return;
  }
  if (command === "index") {
    const index = await fetchText(INDEX_URL);
    printIndex(index, query);
    return;
  }
  if (command === "page") {
    if (!query) fail("Usage: acp-docs.mjs page <protocol/tool-calls|url>");
    const url = toDocUrl(query);
    const text = await fetchText(url);
    printPage(url, text);
    return;
  }
  if (command === "search") {
    if (!query) fail("Usage: acp-docs.mjs search <terms>");
    await searchDocs(query);
    return;
  }
  if (command === "openapi") {
    const text = await fetchText(OPENAPI_URL);
    printPage(OPENAPI_URL, text);
    return;
  }
  fail(`Unknown command: ${command}`);
}

function printHelp() {
  console.log(`ACP docs helper

Commands:
  index [terms]        List official ACP doc pages, optionally filtered.
  page <slug|url>      Fetch one page. Example: page protocol/tool-calls
  search <terms>       Search likely official ACP pages for terms.
  openapi              Fetch official OpenAPI JSON.

Source:
  ${INDEX_URL}`);
}

async function searchDocs(termsText) {
  const terms = termsText.toLowerCase().split(/\s+/).filter(Boolean);
  const index = await fetchText(INDEX_URL);
  const urls = parseUrls(index);
  const candidates = prioritizeUrls(urls, terms).slice(0, 18);
  const hits = [];

  for (const url of candidates) {
    const text = await fetchText(url);
    const lines = text.split(/\r?\n/);
    const matched = [];
    lines.forEach((line, index) => {
      const lower = line.toLowerCase();
      if (terms.every((term) => lower.includes(term)) || terms.some((term) => lower.includes(term))) {
        matched.push({ line: index + 1, text: cleanLine(line) });
      }
    });
    if (matched.length > 0) hits.push({ url, matched: matched.slice(0, 8) });
  }

  if (hits.length === 0) {
    console.log(`No hits for: ${termsText}`);
    console.log("Try: index <one-term>, page protocol/schema, or page protocol/tool-calls");
    return;
  }

  for (const hit of hits.slice(0, 8)) {
    console.log(`\n${hit.url}`);
    for (const match of hit.matched) {
      console.log(`  L${match.line}: ${match.text}`);
    }
  }
}

function printIndex(index, filter) {
  const urls = parseUrls(index);
  const terms = filter.toLowerCase().split(/\s+/).filter(Boolean);
  const filtered = terms.length
    ? urls.filter((url) => terms.some((term) => url.toLowerCase().includes(term)))
    : urls;
  for (const url of filtered) console.log(url);
}

function printPage(url, text) {
  console.log(`# Source: ${url}`);
  text.split(/\r?\n/).forEach((line, index) => {
    console.log(`${String(index + 1).padStart(4, " ")}  ${line}`);
  });
}

function parseUrls(text) {
  return [...text.matchAll(/\((https:\/\/agentclientprotocol\.com\/[^)]+)\)/g)]
    .map((match) => match[1])
    .filter((url, index, all) => all.indexOf(url) === index);
}

function prioritizeUrls(urls, terms) {
  const anchors = [
    "protocol/tool-calls.md",
    "protocol/schema.md",
    "protocol/overview.md",
    "protocol/content.md",
    "protocol/terminals.md",
    "protocol/file-system.md",
    "protocol/session-setup.md",
    "protocol/prompt-turn.md",
  ];
  return [...urls].sort((a, b) => scoreUrl(b, terms, anchors) - scoreUrl(a, terms, anchors));
}

function scoreUrl(url, terms, anchors) {
  let score = anchors.some((anchor) => url.endsWith(anchor)) ? 10 : 0;
  for (const term of terms) if (url.toLowerCase().includes(term)) score += 4;
  if (url.includes("/protocol/")) score += 2;
  if (url.endsWith(".md")) score += 1;
  return score;
}

function toDocUrl(value) {
  if (/^https:\/\/agentclientprotocol\.com\//.test(value)) return value;
  const slug = value.replace(/^\/+/, "").replace(/\.md$/, "");
  return `https://agentclientprotocol.com/${slug}.md`;
}

async function fetchText(url) {
  await mkdir(CACHE_DIR, { recursive: true });
  await pruneCache();
  const cachePath = join(CACHE_DIR, `${createHash("sha256").update(url).digest("hex")}.txt`);
  try {
    const cached = await readFile(cachePath, "utf8");
    const [stamp, ...body] = cached.split("\n");
    const ageMs = Date.now() - Number(stamp.replace("# fetched_at_ms=", ""));
    if (Number.isFinite(ageMs) && ageMs < CACHE_READ_TTL_MS) return body.join("\n");
  } catch {
    // Cache miss.
  }

  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 12000);
  try {
    const response = await fetch(url, {
      headers: { accept: "text/plain, application/json, text/markdown, */*" },
      signal: controller.signal,
    });
    if (!response.ok) fail(`Fetch failed ${response.status} for ${url}`);
    const text = await response.text();
    await writeFile(cachePath, `# fetched_at_ms=${Date.now()}\n${text}`);
    return text;
  } finally {
    clearTimeout(timeout);
  }
}

async function pruneCache() {
  let entries;
  try {
    entries = await readdir(CACHE_DIR, { withFileTypes: true });
  } catch {
    return;
  }

  const now = Date.now();
  const files = [];
  for (const entry of entries) {
    if (!entry.isFile() || !entry.name.endsWith(".txt")) continue;
    const path = join(CACHE_DIR, entry.name);
    try {
      const metadata = await stat(path);
      if (now - metadata.mtimeMs > CACHE_MAX_AGE_MS) {
        await rm(path);
      } else {
        files.push({ path, mtimeMs: metadata.mtimeMs });
      }
    } catch {
      // Ignore files removed by another process.
    }
  }

  files.sort((a, b) => b.mtimeMs - a.mtimeMs);
  await Promise.all(files.slice(CACHE_MAX_FILES).map((file) => rm(file.path).catch(() => {})));
}

function cleanLine(line) {
  return line.replace(/\s+/g, " ").trim().slice(0, 220);
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

main().catch((error) => fail(error instanceof Error ? error.message : String(error)));
