import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const assetRoot = path.dirname(fileURLToPath(import.meta.url));
const manifest = JSON.parse(await readFile(path.join(assetRoot, "session-recovery-manifest.json"), "utf8"));
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
const newline = String.fromCharCode(10);

/** A reviewed patch for exactly one upstream artifact, never a fuzzy upgrade. */
export function patchBundle(original) {
  if (digest(original) !== manifest.upstreamSha256) throw new Error("Unexpected Codex ACP upstream artifact.");
  let source = original.toString();
  const replace = (before, after, count) => {
    if (source.split(before).length - 1 !== count) throw new Error("Codex ACP patch boundary changed.");
    source = source.replaceAll(before, after);
  };
  replace(
    "#!/usr/bin/env node" + newline,
    "#!/usr/bin/env node" + newline + 'import { recoveredSessionMode, resumeNativeSession } from "./openaide-session-recovery.mjs";' + newline,
    1,
  );
  // Merely supplying a provider disables native persisted model/effort restore.
  // Explicit adapter gateway/provider overrides retain their existing behavior.
  replace(
    "modelProvider: await this.getResumeModelProvider()," + newline + "      threadId: request.sessionId",
    "modelProvider: this.getModelProvider() ?? undefined," + newline + "      threadId: request.sessionId",
    2,
  );
  // Native resume reloads the sandbox from current configuration. Resolve the
  // session's persisted policy before that operation can append new settings.
  replace(
    "const response = await this.codexClient.threadResume({",
    "const response = await resumeNativeSession(this.codexClient, {",
    2,
  );
  replace(
    ["sessionId: request.sessionId,", "      currentModelId,", "      models: codexModels,", "      collaborationMode: this.getCollaborationMode(response.thread.id),"].join(newline),
    ["sessionId: request.sessionId,", "      nativePolicy: response,", "      currentModelId,", "      models: codexModels,", "      collaborationMode: this.getCollaborationMode(response.thread.id),"].join(newline),
    2,
  );
  replace("agentMode: AgentMode.getInitialAgentMode(),", "agentMode: recoveredSessionMode(sessionMetadata.nativePolicy, AgentMode),", 2);
  // A restored full profile can contain rules the legacy sandbox projection
  // cannot express. Native Codex retains it until an explicit preset change.
  replace(
    "sandboxPolicy: addAdditionalDirectoriesToSandboxPolicy(agentMode.sandboxPolicy, additionalDirectories),",
    "sandboxPolicy: agentMode.usesNativePermissions ? undefined : addAdditionalDirectoriesToSandboxPolicy(agentMode.sandboxPolicy, additionalDirectories),",
    1,
  );
  replace(
    "applyModeChange(sessionState, value) {" + newline + "    const newMode = AgentMode.find(value);",
    "applyModeChange(sessionState, value) {" + newline + "    if (value === sessionState.agentMode.id) return;" + newline + "    const newMode = AgentMode.find(value);",
    1,
  );
  return source;
}

export async function verifyInstallation(destination) {
  const packageRoot = path.join(destination, "node_modules/@openaide/codex-acp");
  const installed = JSON.parse(await readFile(path.join(packageRoot, "package.json"), "utf8"));
  if (installed.name !== manifest.packageName || installed.version !== manifest.packageVersion) {
    throw new Error("Unexpected Codex ACP package version.");
  }
  const [entrypoint, helper] = await Promise.all([
    readFile(path.join(packageRoot, "dist/index.js")),
    readFile(path.join(packageRoot, "dist/openaide-session-recovery.mjs")),
  ]);
  if (digest(entrypoint) !== manifest.patchedSha256 || digest(helper) !== manifest.helperSha256) {
    throw new Error("Codex ACP recovery artifact validation failed.");
  }
}

async function install(destination) {
  const packageRoot = path.join(destination, "node_modules/@openaide/codex-acp");
  const installed = JSON.parse(await readFile(path.join(packageRoot, "package.json"), "utf8"));
  if (installed.name !== manifest.packageName || installed.version !== manifest.packageVersion) {
    throw new Error("Unexpected Codex ACP package version.");
  }
  const entrypoint = path.join(packageRoot, "dist/index.js");
  const [original, helper] = await Promise.all([
    readFile(entrypoint), readFile(path.join(assetRoot, "session-recovery.mjs")),
  ]);
  const patched = patchBundle(original);
  if (digest(patched) !== manifest.patchedSha256 || digest(helper) !== manifest.helperSha256) {
    throw new Error("Codex ACP recovery patch validation failed.");
  }
  // The provisioner owns an unpublished staging directory; failed writes can
  // never replace an installation being used by another App Server process.
  await writeFile(path.join(packageRoot, "dist/openaide-session-recovery.mjs"), helper);
  await writeFile(entrypoint, patched);
  await verifyInstallation(destination);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [operation, target] = process.argv.slice(2);
  if (!operation) throw new Error("A managed runtime staging directory is required.");
  if (operation === "--verify") await verifyInstallation(target);
  else await install(operation);
}
