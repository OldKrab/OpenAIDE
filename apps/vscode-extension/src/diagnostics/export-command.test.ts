import { beforeEach, describe, expect, it, vi } from "vitest";
import { exportSupportDiagnostics } from "./export";

const vscodeMocks = vi.hoisted(() => ({
  showSaveDialog: vi.fn(),
  showInformationMessage: vi.fn(),
  writeFile: vi.fn(),
  openExternal: vi.fn(),
}));

vi.mock("vscode", () => ({
  version: "1.100.0",
  window: {
    showSaveDialog: vscodeMocks.showSaveDialog,
    showInformationMessage: vscodeMocks.showInformationMessage,
  },
  workspace: {
    fs: { writeFile: vscodeMocks.writeFile },
  },
  env: { openExternal: vscodeMocks.openExternal },
  Uri: {
    file: (fsPath: string) => ({ fsPath }),
    parse: (value: string) => ({ value }),
  },
}));

describe("Support Export command", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("saves one ZIP and offers the prefilled GitHub bug report", async () => {
    vscodeMocks.showSaveDialog.mockResolvedValue({ fsPath: "/tmp/openaide-support.zip" });
    vscodeMocks.showInformationMessage.mockResolvedValue("Open GitHub Bug Report");
    const runtime = {
      async appServerRequest() {
        return {
          status: "ready",
          version: "0.1.0",
          methodCount: 13,
          tasks: {
            visibleCount: 0,
            totalCount: 0,
            activeCount: 0,
            activeTasks: [],
            revision: 0,
          },
          redaction: "prompt_text_file_contents_terminal_output_and_secrets_removed" as const,
        };
      },
    };
    const runtimeProcess = {
      describe() {
        return {
          running: true,
          runtime_source_kind: "bundled" as const,
          storage_root_kind: "extension-storage" as const,
        };
      },
      describeSupportHost() {
        return {
          diagnostics_log_directory: "/missing/logs",
          extension_version: "0.0.1-alpha.4",
        };
      },
    };
    const supportExportState = {
      get: vi.fn(() => "/remembered/reports"),
      update: vi.fn(async () => undefined),
    };

    await exportSupportDiagnostics(runtime as never, runtimeProcess as never, supportExportState);

    expect(vscodeMocks.showSaveDialog).toHaveBeenCalledWith(expect.objectContaining({
      filters: { "ZIP archive": ["zip"] },
    }));
    const saveOptions = vscodeMocks.showSaveDialog.mock.calls[0]?.[0];
    expect(saveOptions.defaultUri.fsPath).toMatch(/^\/remembered\/reports\/openaide-support-.*\.zip$/);
    expect(supportExportState.update).toHaveBeenCalledWith(
      "openaide.supportExport.lastDirectory",
      "/tmp",
    );
    const bytes = vscodeMocks.writeFile.mock.calls[0]?.[1] as Uint8Array;
    expect(Buffer.from(bytes).readUInt32LE(0)).toBe(0x04034b50);
    expect(vscodeMocks.showInformationMessage).toHaveBeenCalledWith(
      expect.stringContaining("Review it before attaching"),
      "Open GitHub Bug Report",
    );
    expect(vscodeMocks.openExternal).toHaveBeenCalledWith(expect.objectContaining({
      value: expect.stringContaining("template=bug_report.yml"),
    }));
  });

  it("still opens the save dialog when runtime diagnostics never settles", async () => {
    vi.useFakeTimers();
    try {
      vscodeMocks.showSaveDialog.mockResolvedValue(undefined);
      const exporting = exportSupportDiagnostics(
        {
          appServerRequest: () => new Promise(() => undefined),
        } as never,
        {
          describe: () => ({
            running: true,
            runtime_source_kind: "bundled" as const,
            storage_root_kind: "extension-storage" as const,
          }),
          describeSupportHost: () => ({
            diagnostics_log_directory: "/missing/logs",
            extension_version: "0.0.1-alpha.4",
          }),
        } as never,
      );

      await vi.advanceTimersByTimeAsync(5_000);
      await exporting;

      expect(vscodeMocks.showSaveDialog).toHaveBeenCalledOnce();
    } finally {
      vi.useRealTimers();
    }
  });
});
