// The Web Shell's own settings: its ports, roots, hosts and App Server binary.
const WEB_SHELL_SETTING = /^OPENAIDE_WEB_/;
const WEB_SHELL_SETTINGS = new Set(["OPENAIDE_APP_SERVER_PATH", "OPENAIDE_RUNTIME_PATH", "OPENAIDE_PROJECT_ROOTS"]);

/**
 * The environment for the App Server this Web Shell starts. The App Server
 * passes what it inherits to Agents and Terminal shells, so the Web Shell's own
 * settings stay here: a Web Shell or smoke run started by an Agent must not
 * pick up this instance's ports and roots. The App Server takes its own
 * settings out of its environment itself (openaide-rs/app-server/src/instance_env.rs).
 */
export function appServerEnvironment(parentEnv, { stateRoot, runtimeRoot }) {
  const env = Object.fromEntries(
    Object.entries(parentEnv).filter(([name]) => !WEB_SHELL_SETTING.test(name) && !WEB_SHELL_SETTINGS.has(name)),
  );
  const projectRoots = parentEnv.OPENAIDE_WEB_PROJECT_ROOTS;
  return {
    ...env,
    ...(projectRoots ? { OPENAIDE_PROJECT_ROOTS: projectRoots } : {}),
    OPENAIDE_STORAGE_ROOT: stateRoot,
    OPENAIDE_RUNTIME_ROOT: runtimeRoot,
    OPENAIDE_APP_SERVER_PROTOCOL: "app-server-handoff",
  };
}
