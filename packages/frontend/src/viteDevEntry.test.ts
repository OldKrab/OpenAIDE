import { mkdtemp, rm } from "node:fs/promises";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { expect, test } from "vitest";

const packageRoot = fileURLToPath(new URL("..", import.meta.url));
const configFile = path.join(packageRoot, "vite.config.ts");

async function fetchModule(url: URL): Promise<string> {
  // Vite answers a page fallback unless the request says it wants a script.
  const response = await fetch(url, { headers: { Accept: "*/*", "Sec-Fetch-Dest": "script" } });
  expect(response.status, url.pathname).toBe(200);
  expect(response.headers.get("content-type"), url.pathname).toMatch(/javascript/);
  return response.text();
}

test("the dev server serves the Browser Shell entry that its page loads", async () => {
  const cacheDir = await mkdtemp(path.join(tmpdir(), "openaide-vite-dev-"));
  const server = await createServer({
    root: packageRoot,
    configFile,
    cacheDir,
    logLevel: "silent",
    optimizeDeps: { noDiscovery: true },
    server: { host: "127.0.0.1", port: 0, strictPort: true, watch: null, hmr: false },
  });

  try {
    await server.listen();
    const { port } = server.httpServer!.address() as AddressInfo;
    const origin = `http://127.0.0.1:${port}/`;

    const page = await fetch(origin, { headers: { Accept: "text/html" } });
    const scripts = [...(await page.text()).matchAll(/<script type="module" src="([^"]+)"/g)]
      .map(([, src]) => new URL(src, origin))
      .filter((url) => !url.pathname.startsWith("/@"));
    expect(scripts).toHaveLength(1);

    // The shell entry sits outside Vite's root, so only an import resolves against index.html's
    // own location: a script src resolves against the page URL and misses the entry in dev.
    const entry = await fetchModule(scripts[0]);
    const shellEntry = entry.match(/"([^"]*apps\/browser\/frontend\/main\.tsx[^"]*)"/)?.[1];
    expect(shellEntry).toBeDefined();
    await fetchModule(new URL(shellEntry!, origin));
  } finally {
    await server.close();
    await rm(cacheDir, { recursive: true, force: true });
  }
});
