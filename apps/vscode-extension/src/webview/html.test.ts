import { describe, expect, it, vi } from "vitest";
import { renderWebviewHtml } from "./html";
import { VSCODE_SHELL } from "./types";

vi.mock("vscode", () => ({
  Uri: {
    joinPath: (base: { fsPath: string }, ...parts: string[]) => ({
      fsPath: [base.fsPath, ...parts].join("/"),
    }),
  },
}));

describe("webview html", () => {
  it("allows bundled frontend fonts in the webview CSP", () => {
    const html = renderWebviewHtml(context(), webview(), {
      surface: "navigation",
      shell: VSCODE_SHELL,
    });

    expect(html).toContain("font-src vscode-webview:;");
    expect(html).toContain('data-shell="vscodeExtension"');
    expect(html).toContain('data-navigation-mode="currentProject"');
  });

  it("embeds LocalHttp bootstrap info and allows that origin in CSP", () => {
    const html = renderWebviewHtml(context(), webview(), {
      surface: "navigation",
      shell: VSCODE_SHELL,
      appServerConnection: {
        kind: "localHttp",
        endpointUrl: "http://127.0.0.1:4321/probe",
        authToken: "token-1",
      },
    });

    expect(html).toContain("connect-src http://127.0.0.1:4321;");
    expect(html).toContain("data-app-server-connection=");
    expect(html).toContain("&quot;kind&quot;:&quot;localHttp&quot;");
    expect(html).toContain("&quot;authToken&quot;:&quot;token-1&quot;");
  });
});

function context() {
  return {
    extensionUri: { fsPath: "/extension" },
  } as never;
}

function webview() {
  return {
    cspSource: "vscode-webview:",
    asWebviewUri: (uri: { fsPath: string }) => `vscode-webview://${uri.fsPath}`,
  } as never;
}
