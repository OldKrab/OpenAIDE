import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { injectBootstrap, webRoute } from "./dev-server-routes.mjs";
import { createViteProxy } from "./dev-server-vite-proxy.mjs";
import { createRuntimeLogger } from "./runtime-logger.mjs";
import { createStaticFrontend } from "./static-frontend.mjs";
import { startWebServer, webServerEnvironment } from "./web-server.mjs";

// Development entry point: serves the Frontend through Vite, or from a built
// static root, and can route `/prototype` to a prototype Vite server.
// TODO: the Android Termux runtime still launches this entry with a static
// root; point it at server.mjs so packaged installs share one entry point.
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const environment = webServerEnvironment();
const vitePort = Number(process.env.OPENAIDE_WEB_VITE_PORT ?? "5173");
const prototypePort = process.env.OPENAIDE_WEB_PROTOTYPE_PORT
  ? Number(process.env.OPENAIDE_WEB_PROTOTYPE_PORT)
  : undefined;
const staticRoot = process.env.OPENAIDE_WEB_STATIC_ROOT
  ? path.resolve(process.env.OPENAIDE_WEB_STATIC_ROOT)
  : undefined;
const logger = createRuntimeLogger("openaide-web-server");

if (prototypePort !== undefined && (!Number.isInteger(prototypePort) || prototypePort < 1 || prototypePort > 65_535)) {
  throw new Error("OPENAIDE_WEB_PROTOTYPE_PORT must be a valid TCP port.");
}

const vite = staticRoot ? undefined : spawn(
  process.platform === "win32" ? "npm.cmd" : "npm",
  ["exec", "--workspace", "openaide-frontend", "vite", "--", ...viteArgs()],
  {
    cwd: repoRoot,
    env: {
      ...process.env,
      OPENAIDE_VITE_ALLOWED_HOSTS: [environment.host, ...environment.allowedHosts].filter(Boolean).join(","),
    },
    stdio: ["ignore", "inherit", "inherit"],
  },
);

await startWebServer({
  ...environment,
  appServerCwd: repoRoot,
  appServerPath: path.resolve(
    process.env.OPENAIDE_APP_SERVER_PATH
      ?? process.env.OPENAIDE_RUNTIME_PATH
      ?? path.join(repoRoot, "target", "debug", "openaide-app-server"),
  ),
  frontend: staticRoot
    ? createStaticFrontend({ root: staticRoot, presentation: environment.presentation, logger })
    : createViteProxy({
        port: vitePort,
        transformResponse: ({ body, headers, url }) => {
          const route = webRoute(url.pathname);
          if (route && headers["content-type"]?.includes("text/html")) {
            body = Buffer.from(injectBootstrap(body.toString("utf8"), route, environment.presentation), "utf8");
          }
          return { body, headers };
        },
      }),
  logger,
  name: "OpenAIDE Web dev shell",
  onShutdown: (signal) => vite?.kill(signal),
  port: Number(process.env.OPENAIDE_WEB_PORT ?? "5174"),
  prototype: prototypePort === undefined
    ? undefined
    : createViteProxy({
        port: prototypePort,
        unavailableMessage: "Prototype server is not running. Start it with npm run prototype:target.",
      }),
  runtimeRoot: path.resolve(process.env.OPENAIDE_WEB_RUNTIME_ROOT ?? path.join(repoRoot, ".openaide-web-dev", "runtime")),
  stateRoot: path.resolve(process.env.OPENAIDE_WEB_STATE_ROOT ?? path.join(repoRoot, ".openaide-web-dev", "state")),
});

function viteArgs() {
  const args = ["--host", "127.0.0.1", "--port", String(vitePort)];
  if (process.env.OPENAIDE_VITE_CONFIG) {
    args.push("--config", process.env.OPENAIDE_VITE_CONFIG);
  }
  return args;
}
