import { beforeEach, describe, expect, it, vi } from "vitest";
import { installFrontendShell } from "../services/frontendShell";
import { copyText } from "./clipboard";

describe("copyText", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("uses the App Shell clipboard capability instead of the webview clipboard", async () => {
    const shellWriteText = vi.fn().mockResolvedValue(undefined);
    const webviewWriteText = vi.fn().mockRejectedValue(new DOMException("Document is not focused.", "NotAllowedError"));
    vi.stubGlobal("navigator", { clipboard: { writeText: webviewWriteText } });
    installFrontendShell({ clipboard: { writeText: shellWriteText } } as never);

    await copyText("copy me");

    expect(shellWriteText).toHaveBeenCalledWith("copy me");
    expect(webviewWriteText).not.toHaveBeenCalled();
  });
});
