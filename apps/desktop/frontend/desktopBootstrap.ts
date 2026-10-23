export type LocalHttpConnection = {
  kind: "localHttp";
  endpointUrl: string;
  authToken: string;
};

/**
 * Addresses another App Server HTTP route beside the RPC endpoint, which ends in `/probe`.
 * A paired computer's gateway keeps the same layout under its own base path.
 */
export function localHttpRouteUrl(endpointUrl: string, route: "download" | "upload" | "terminal") {
  let url: URL;
  try {
    url = new URL(endpointUrl);
  } catch {
    return undefined;
  }
  const prefix = url.pathname.replace(/\/+$/, "");
  if (!prefix.endsWith("/probe")) return undefined;
  url.pathname = `${prefix.slice(0, -"/probe".length)}/${route}`;
  url.search = "";
  url.hash = "";
  return url;
}

export type DesktopRuntimeEnvironment =
  | { kind: "native" }
  | { kind: "wsl"; distro: string };

export type DesktopRuntimeOptions = {
  wslDistros: string[];
};

export type DesktopBootstrap = {
  clientInstanceId: string;
  connection: LocalHttpConnection;
  platform: "linux" | "macos" | "windows";
  runtimeEnvironment: DesktopRuntimeEnvironment;
  runtimeOptions: DesktopRuntimeOptions;
  /** Names the paired computer whose App Server is in use instead of this one's. */
  remoteComputer?: string;
  /** The computer in use is an SSH Computer, whose App Server this shell restores (ADR-0069). */
  sshComputer?: boolean;
};

export type DesktopBootstrapProgress = {
  message: string;
  stage: string;
};

export type DesktopBootstrapContext = {
  environment: DesktopRuntimeEnvironment;
  canRecover: boolean;
  remoteComputer?: string;
};

type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
type Listen = (
  event: string,
  listener: (event: { payload: DesktopBootstrapProgress }) => void,
) => Promise<() => void>;

/** Observes shell-owned startup before requesting the App Server handoff. */
export async function runDesktopBootstrap({
  invoke,
  listen,
  onProgress,
  onContext = () => undefined,
}: {
  invoke: Invoke;
  listen: Listen;
  onProgress(progress: DesktopBootstrapProgress): void;
  onContext?(context: DesktopBootstrapContext): void;
}): Promise<DesktopBootstrap> {
  const stop = await listen("desktop-bootstrap-progress", ({ payload }) => onProgress(payload));
  try {
    const context = await invoke("desktop_bootstrap_context") as DesktopBootstrapContext;
    onContext(context);
    return await invoke("desktop_bootstrap") as DesktopBootstrap;
  } finally {
    stop();
  }
}
