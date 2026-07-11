import { existsSync, readdirSync, statSync } from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import type { EnvVariable } from "./hostTerminalTypes";

const CODEX_TOOL_DIR = "codex-path";

export function terminalEnvironment(requestEnv: EnvVariable[]): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env };
  applyEnvEntries(env, terminalIntegratedEnv(env));
  applyEnvEntries(env, requestEnv);

  const key = pathEnvKey(env);
  const value = env[key];
  if (value) {
    env[key] = normalizedTerminalPath(value, env.HOME);
  }
  return env;
}

function applyEnvEntries(env: NodeJS.ProcessEnv, entries: EnvVariable[]) {
  for (const entry of entries) {
    env[entry.name] = entry.value;
  }
}

function terminalIntegratedEnv(env: NodeJS.ProcessEnv): EnvVariable[] {
  const platform = process.platform === "win32" ? "windows" : process.platform === "darwin" ? "osx" : "linux";
  const configured = vscode.workspace.getConfiguration("terminal.integrated").get<Record<string, string | null>>(`env.${platform}`);
  if (!configured || typeof configured !== "object" || Array.isArray(configured)) return [];

  return Object.entries(configured).flatMap(([name, value]) => {
    if (typeof value !== "string") return [];
    return [{ name, value: expandEnvValue(value, env) }];
  });
}

function expandEnvValue(value: string, env: NodeJS.ProcessEnv) {
  return value.replace(/\$\{env:([^}]+)\}/g, (_match, name: string) => env[name] ?? "");
}

function normalizedTerminalPath(value: string, home: string | undefined) {
  const entries = value.split(path.delimiter).filter(Boolean);
  const repairedEntries = entries.flatMap((entry) => [entry, ...codexToolPathForEntry(entry)]);
  return dedupePathEntries([...repairedEntries, ...discoveredCodexToolPaths(home)]).join(path.delimiter);
}

function codexToolPathForEntry(entry: string) {
  if (path.basename(entry) !== "path") return [];

  const sibling = path.join(path.dirname(entry), CODEX_TOOL_DIR);
  return isDirectory(sibling) ? [sibling] : [];
}

function discoveredCodexToolPaths(home: string | undefined) {
  if (!home) return [];

  const nodeRoots = [
    ...childDirectories(path.join(home, ".nvm", "versions", "node")).map((version) => path.join(version, "lib", "node_modules")),
    path.join(home, ".npm-global", "lib", "node_modules"),
    path.join(home, ".local", "lib", "node_modules"),
    "/usr/local/lib/node_modules",
    "/usr/lib/node_modules",
  ];

  return nodeRoots.flatMap((root) => {
    const codexNodeModules = path.join(root, "@openai", "codex", "node_modules");
    return nodePackageDirectories(codexNodeModules).flatMap((packageDir) => {
      if (!path.basename(packageDir).startsWith("codex-")) return [];
      return childDirectories(path.join(packageDir, "vendor")).flatMap((targetDir) => {
        const candidate = path.join(targetDir, CODEX_TOOL_DIR);
        return isDirectory(candidate) ? [candidate] : [];
      });
    });
  });
}

function nodePackageDirectories(nodeModules: string) {
  return childDirectories(nodeModules).flatMap((packageOrScope) => {
    return path.basename(packageOrScope).startsWith("@") ? childDirectories(packageOrScope) : [packageOrScope];
  });
}

function pathEnvKey(env: NodeJS.ProcessEnv) {
  if (process.platform !== "win32") return "PATH";
  return Object.keys(env).find((key) => key.toLowerCase() === "path") ?? "Path";
}

function dedupePathEntries(entries: string[]) {
  const seen = new Set<string>();
  return entries.filter((entry) => {
    const key = process.platform === "win32" ? entry.toLowerCase() : entry;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function childDirectories(parent: string) {
  try {
    return readdirSync(parent, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .map((entry) => path.join(parent, entry.name));
  } catch {
    return [];
  }
}

function isDirectory(filePath: string) {
  try {
    return existsSync(filePath) && statSync(filePath).isDirectory();
  } catch {
    return false;
  }
}
