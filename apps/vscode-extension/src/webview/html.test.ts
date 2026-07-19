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

  it("allows inline image previews in the webview CSP", () => {
    const html = renderWebviewHtml(context(), webview(), {
      surface: "task",
      shell: VSCODE_SHELL,
    });

    expect(html).toContain("img-src data:;");
  });

  it("never exposes App Server endpoint or token material to a VS Code webview", () => {
    const html = renderWebviewHtml(context(), webview(), {
      surface: "navigation",
      shell: VSCODE_SHELL,
      appServerConnection: {
        kind: "localHttp",
        endpointUrl: "http://127.0.0.1:4321/probe",
        authToken: "token-1",
      },
    });

    expect(html).not.toContain("connect-src http://127.0.0.1:4321;");
    expect(html).not.toContain("data-app-server-connection=");
    expect(html).not.toContain("token-1");
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
