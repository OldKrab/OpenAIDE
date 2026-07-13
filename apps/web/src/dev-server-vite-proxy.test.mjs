import assert from "node:assert/strict";
import http from "node:http";
import test from "node:test";
import { createViteProxy } from "./dev-server-vite-proxy.mjs";

test("Vite proxy returns a transformed response", async (t) => {
  const upstream = http.createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html" });
    response.end("<main>Original</main>");
  });
  const upstreamPort = await listen(upstream);
  const proxy = createViteProxy({
    port: upstreamPort,
    transformResponse: ({ body, headers }) => ({
      body: Buffer.from(body.toString("utf8").replace("Original", "Transformed")),
      headers: { ...headers, "x-transformed": "yes" },
    }),
  });
  const server = http.createServer(async (request, response) => {
    try {
      await proxy.request(request, response, new URL(request.url ?? "/", "http://localhost"));
    } catch (error) {
      response.writeHead(502, { "content-type": "text/plain" });
      response.end(error instanceof Error ? error.message : String(error));
    }
  });
  const proxyPort = await listen(server);
  t.after(async () => Promise.all([close(server), close(upstream)]));

  const response = await fetch(`http://127.0.0.1:${proxyPort}/task/example`);

  assert.equal(response.status, 200);
  assert.equal(response.headers.get("x-transformed"), "yes");
  assert.equal(await response.text(), "<main>Transformed</main>");
});

function listen(server) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });
}

function close(server) {
  if (!server.listening) return Promise.resolve();
  return new Promise((resolve, reject) => {
    server.close((error) => error ? reject(error) : resolve());
  });
}
