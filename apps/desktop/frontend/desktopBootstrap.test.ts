import { describe, expect, it, vi } from "vitest";
import { runDesktopBootstrap, type DesktopBootstrapProgress } from "./desktopBootstrap";

describe("Desktop startup", () => {
  it("shows runtime preparation progress before returning the connected backend", async () => {
    let publish: ((event: { payload: DesktopBootstrapProgress }) => void) | undefined;
    const unsubscribe = vi.fn();
    const listen = vi.fn(async (_event, listener) => {
      publish = listener;
      return unsubscribe;
    });
    const invoke = vi.fn(async (command: string) => {
      if (command === "desktop_bootstrap_context") {
        return { environment: { kind: "wsl", distro: "Ubuntu" }, canRecover: true };
      }
      publish?.({ payload: { message: "Installing OpenAIDE runtime in Ubuntu", stage: "wsl_install" } });
      publish?.({ payload: { message: "Starting OpenAIDE in Ubuntu", stage: "wsl_launch" } });
      return connectedBootstrap();
    });
    const messages: string[] = [];
    const contexts: string[] = [];

    const bootstrap = await runDesktopBootstrap({
      invoke,
      listen,
      onProgress: (progress) => messages.push(progress.message),
      onContext: (context) => contexts.push(context.environment.kind),
    });

    expect(messages).toEqual([
      "Installing OpenAIDE runtime in Ubuntu",
      "Starting OpenAIDE in Ubuntu",
    ]);
    expect(bootstrap.runtimeEnvironment).toEqual({ kind: "wsl", distro: "Ubuntu" });
    expect(contexts).toEqual(["wsl"]);
    expect(unsubscribe).toHaveBeenCalledOnce();
  });
});

function connectedBootstrap() {
  return {
    clientInstanceId: "desktop-1",
    connection: {
      kind: "localHttp" as const,
      endpointUrl: "http://127.0.0.1:4567/probe",
      authToken: "a".repeat(64),
    },
    platform: "windows" as const,
    runtimeEnvironment: { kind: "wsl" as const, distro: "Ubuntu" },
    runtimeOptions: { wslDistros: ["Ubuntu"] },
  };
}
