import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("Extension Development Host launch", () => {
  it("isolates OpenAIDE storage from the installed extension", () => {
    const launchConfig = JSON.parse(
      readFileSync(new URL("../../../../.vscode/launch.json", import.meta.url), "utf8"),
    ) as { configurations?: Array<{ name?: string; env?: Record<string, string> }> };
    const extensionHost = launchConfig.configurations?.find(
      (configuration) => configuration.name === "OpenAIDE: Extension Host",
    );

    expect(extensionHost?.env?.OPENAIDE_STORAGE_ROOT).toBe(
      "${workspaceFolder}/.vscode-test/openaide-storage",
    );
  });
});
