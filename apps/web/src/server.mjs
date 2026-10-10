import { homedir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createRuntimeLogger } from "./runtime-logger.mjs";
import { createStaticFrontend } from "./static-frontend.mjs";
import { startWebServer, webServerEnvironment } from "./web-server.mjs";

// Packaged entry point: serves only the built Frontend and the App Server
// proxy. Defaults assume the release layout, which mirrors the repository so
// this file resolves the same siblings in a checkout and in an install.
const installRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
// User data stays outside the install so replacing it never touches Task history.
const dataRoot = path.join(process.env.XDG_DATA_HOME || path.join(homedir(), ".local", "share"), "openaide-web");
const environment = webServerEnvironment();
const logger = createRuntimeLogger("openaide-web-server");

await startWebServer({
  ...environment,
  appServerCwd: homedir(),
  appServerPath: path.resolve(
    process.env.OPENAIDE_APP_SERVER_PATH ?? path.join(installRoot, "bin", "openaide-app-server"),
  ),
  frontend: createStaticFrontend({
    root: path.resolve(process.env.OPENAIDE_WEB_STATIC_ROOT ?? path.join(installRoot, "packages", "frontend", "dist")),
    presentation: environment.presentation,
    logger,
  }),
  logger,
  name: "OpenAIDE Web",
  port: Number(process.env.OPENAIDE_WEB_PORT ?? "5474"),
  runtimeRoot: path.resolve(process.env.OPENAIDE_WEB_RUNTIME_ROOT ?? path.join(dataRoot, "runtime")),
  stateRoot: path.resolve(process.env.OPENAIDE_WEB_STATE_ROOT ?? path.join(dataRoot, "state")),
});
