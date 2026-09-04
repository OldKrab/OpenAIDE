import { beforeEach, describe, expect, it, vi } from "vitest";
import { registerCommands } from "./index";

const vscodeMocks = vi.hoisted(() => ({
  handlers: new Map<string, () => unknown>(),
  registerCommand: vi.fn((name: string, handler: () => unknown) => {
    vscodeMocks.handlers.set(name, handler);
    return { dispose: vi.fn() };
  }),
  showInformationMessage: vi.fn(),
}));

vi.mock("vscode", () => ({
  commands: { registerCommand: vscodeMocks.registerCommand },
  window: { showInformationMessage: vscodeMocks.showInformationMessage },
}));

describe("VS Code commands", () => {
  beforeEach(() => {
    vscodeMocks.handlers.clear();
    vi.clearAllMocks();
  });

  it("opens the diagnostics export wizard through the shared Settings intent", async () => {
    const openSettings = vi.fn();
    registerCommands(
      { subscriptions: [] } as never,
      { openNewTask: vi.fn(), openSettings } as never,
      { health: vi.fn(), appServerRequest: vi.fn() } as never,
    );

    await vscodeMocks.handlers.get("openaide.exportDiagnostics")?.();

    expect(openSettings).toHaveBeenCalledWith(
      undefined,
      undefined,
      undefined,
      "data",
      { kind: "openSupportExport", requestId: expect.any(String) },
    );
  });
});
