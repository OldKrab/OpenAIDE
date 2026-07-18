import { homedir } from "node:os";
import * as path from "node:path";
import * as vscode from "vscode";
import { RuntimeProcess } from "../runtime/process";
import { RuntimeClient } from "../runtime/rpcClient";
import { buildSupportBundle } from "./bundle";
import { collectDiagnostics } from "./snapshot";

const GITHUB_BUG_REPORT_URL = "https://github.com/OldKrab/OpenAIDE/issues/new";

/** Saves a public-safe Support Export and offers the project's bug-report form. */
export async function exportSupportDiagnostics(runtime: RuntimeClient, runtimeProcess: RuntimeProcess) {
  const snapshot = await collectDiagnostics(runtime, runtimeProcess);
  const supportHost = runtimeProcess.describeSupportHost();
  const bundle = await buildSupportBundle({
    snapshot,
    diagnosticsLogDirectory: supportHost.diagnostics_log_directory,
    environment: {
      platform: process.platform,
      architecture: process.arch,
      vscode_version: vscode.version,
      extension_version: supportHost.extension_version,
    },
  });
  const target = await vscode.window.showSaveDialog({
    defaultUri: vscode.Uri.file(path.join(homedir(), supportBundleFileName(new Date(bundle.manifest.created_at)))),
    filters: { "ZIP archive": ["zip"] },
    saveLabel: "Save Support Bundle",
    title: "Export OpenAIDE Support Diagnostics",
  });
  if (!target) return;

  await vscode.workspace.fs.writeFile(target, bundle.bytes);
  const action = await vscode.window.showInformationMessage(
    "OpenAIDE support bundle saved. Review it before attaching it to a public issue.",
    "Open GitHub Bug Report",
  );
  if (action === "Open GitHub Bug Report") {
    const query = new URLSearchParams({
      template: "bug_report.yml",
      version: supportHost.extension_version,
      shell: "VS Code Extension",
    });
    await vscode.env.openExternal(vscode.Uri.parse(`${GITHUB_BUG_REPORT_URL}?${query.toString()}`));
  }
}

function supportBundleFileName(createdAt: Date) {
  const timestamp = createdAt.toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
  return `openaide-support-${timestamp}.zip`;
}
