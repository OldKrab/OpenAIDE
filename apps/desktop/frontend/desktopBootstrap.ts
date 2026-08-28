export type LocalHttpConnection = {
  kind: "localHttp";
  endpointUrl: string;
  authToken: string;
};

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
};

export type DesktopBootstrapProgress = {
  message: string;
  stage: string;
};

export type DesktopBootstrapContext = {
  environment: DesktopRuntimeEnvironment;
  canRecover: boolean;
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
