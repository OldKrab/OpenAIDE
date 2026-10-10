import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { stageWebPackage } from "../../../scripts/package-web.mjs";

// Runs with the workspace tests, after the TypeScript packages it stages are built.
test("the staged Web package starts from its own files and omits development inputs", { timeout: 60_000 }, async (t) => {
  const fixture = mkdtempSync(path.join(tmpdir(), "openaide-web-package-"));
  t.after(() => rmSync(fixture, { recursive: true, force: true }));
  const root = createSourceTree(path.join(fixture, "source"));
  const output = path.join(fixture, "openaide-web");

  stageWebPackage({ appServerPath: path.join(root, "app-server.mjs"), output, root, version: "1.2.3" });

  assert.equal(JSON.parse(readFileSync(path.join(output, "package.json"), "utf8")).version, "1.2.3");
  assert.ok(existsSync(path.join(output, "packages/frontend/dist/assets/index.js")));
  assert.ok(!existsSync(path.join(output, "packages/frontend/dist/assets/index.js.map")));
  assert.ok(!existsSync(path.join(output, "apps/web/src/server.test.mjs")));
  assert.ok(!existsSync(path.join(output, "apps/web/src/dev-server-test-support.mjs")));
  for (const shipped of ["README.md", "LICENSE", "openaide-web.service"]) {
    assert.ok(existsSync(path.join(output, shipped)), shipped);
  }

  // The launcher runs outside the repository, so every import must resolve inside the package.
  const {
    OPENAIDE_APP_SERVER_PATH: _appServerPath,
    OPENAIDE_WEB_RUNTIME_ROOT: _runtimeRoot,
    OPENAIDE_WEB_STATE_ROOT: _stateRoot,
    OPENAIDE_WEB_STATIC_ROOT: _staticRoot,
    ...baseEnv
  } = process.env;
  const server = spawn(path.join(output, "bin/openaide-web"), [], {
    cwd: fixture,
    env: {
      ...baseEnv,
      OPENAIDE_WEB_HOST: "127.0.0.1",
      OPENAIDE_WEB_PORT: "0",
      XDG_DATA_HOME: path.join(fixture, "data"),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(() => {
    if (server.exitCode !== null) return undefined;
    return new Promise((resolve) => {
      server.once("exit", resolve);
      server.kill("SIGTERM");
    });
  });
  const origin = `http://127.0.0.1:${await listeningPort(server)}`;

  assert.match(await (await fetch(origin)).text(), /data-shell="web"/);
  assert.equal((await fetch(`${origin}/assets/index.js`)).status, 200);
  const rpc = await fetch(`${origin}/__openaide-app-server/probe`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: "probe", method: "client/probe", params: {} }),
  });
  assert.equal(rpc.status, 200);
  assert.ok(existsSync(path.join(fixture, "data/openaide-web/state")));
});

// Real server sources and client build, with a stand-in Frontend and App Server.
function createSourceTree(root) {
  const repoRoot = path.resolve(import.meta.dirname, "../../..");
  for (const directory of ["apps/web", "packages/frontend/dist/assets", "deploy"]) {
    mkdirSync(path.join(root, directory), { recursive: true });
  }
  for (const [link, target] of [
    ["apps/web/src", "apps/web/src"],
    ["packages/app-server-client", "packages/app-server-client"],
    ["packages/app-shell-contracts", "packages/app-shell-contracts"],
    ["deploy/web", "deploy/web"],
    ["LICENSE", "LICENSE"],
  ]) {
    copyInput(path.join(repoRoot, target), path.join(root, link));
  }
  const dist = path.join(root, "packages/frontend/dist");
  writeFileSync(path.join(dist, "index.html"), "<html><body><div id=\"root\"></div></body></html>");
  writeFileSync(path.join(dist, "assets/index.js"), "globalThis.openaide = true;");
  writeFileSync(path.join(dist, "assets/index.js.map"), "{}");
  writeFileSync(path.join(root, "app-server.mjs"), `#!/usr/bin/env node
import http from "node:http";
const server = http.createServer((request, response) => {
  const chunks = [];
  request.on("data", (chunk) => chunks.push(chunk));
  request.on("end", () => {
    const message = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ jsonrpc: "2.0", id: message.id, result: {} }));
  });
});
server.listen(0, "127.0.0.1", () => console.log(JSON.stringify({
  kind: "localHttp",
  endpointUrl: \`http://127.0.0.1:\${server.address().port}/rpc\`,
  authToken: "test-token-that-is-long-enough-for-handoff",
})));
process.once("SIGTERM", () => server.close(() => process.exit(0)));
`);
  return root;
}

// A workspace package's own node_modules is never a package input.
function copyInput(source, destination) {
  cpSync(source, destination, { recursive: true, filter: (entry) => path.basename(entry) !== "node_modules" });
}

function listeningPort(child) {
  return new Promise((resolve, reject) => {
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
      const listening = / listening on http:\/\/[^\s:]+:(\d+)\r?\n/.exec(stdout);
      if (listening) resolve(Number(listening[1]));
    });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.once("error", reject);
    child.once("exit", (code) => reject(new Error(`Packaged Web server exited with ${code} before listening: ${stderr}`)));
  });
}
