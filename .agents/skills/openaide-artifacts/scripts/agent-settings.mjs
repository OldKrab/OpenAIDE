import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const HOME = os.homedir();

/** Reads current and historical OpenAIDE Agent settings from supported VS Code profiles. */
export function readAgentSettingsRecords() {
  const records = [];
  for (const file of findSettingsFiles()) {
    const parsed = readJsoncMaybe(file).value;
    const agents = parsed?.["openaide.agents"];
    if (!Array.isArray(agents)) continue;
    records.push({
      file,
      history: file.includes(`${path.sep}History${path.sep}`),
      agents,
    });
  }
  return records;
}

function findSettingsFiles() {
  const roots = [
    path.join(HOME, ".vscode-server", "data", "User"),
    path.join(HOME, ".vscode-server-insiders", "data", "User"),
    path.join(HOME, ".config", "Code", "User"),
    path.join(HOME, ".config", "Code - Insiders", "User"),
  ];
  const files = [];
  for (const root of roots) {
    for (const file of [path.join(root, "settings.json"), path.join(root, "Machine", "settings.json")]) {
      if (fs.existsSync(file)) files.push(file);
    }
    const historyRoot = path.join(root, "History");
    if (!fs.existsSync(historyRoot)) continue;
    for (const dir of fs.readdirSync(historyRoot)) {
      const fullDir = path.join(historyRoot, dir);
      if (!statMaybe(fullDir)?.isDirectory()) continue;
      for (const entry of fs.readdirSync(fullDir)) {
        const file = path.join(fullDir, entry);
        if (statMaybe(file)?.isFile() && entry.endsWith(".json")) files.push(file);
      }
    }
  }
  return files;
}

function readJsoncMaybe(file) {
  if (!fs.existsSync(file)) return { value: undefined };
  try {
    return { value: JSON.parse(stripJsonComments(fs.readFileSync(file, "utf8")).replace(/,\s*([}\]])/g, "$1")) };
  } catch (error) {
    return { error: `${file}: ${error.message}` };
  }
}

function stripJsonComments(text) {
  let output = "";
  let inString = false;
  let escaped = false;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    const next = text[index + 1];
    if (inString) {
      output += char;
      if (escaped) escaped = false;
      else if (char === "\\") escaped = true;
      else if (char === "\"") inString = false;
      continue;
    }
    if (char === "\"") {
      inString = true;
      output += char;
      continue;
    }
    if (char === "/" && next === "/") {
      while (index < text.length && text[index] !== "\n") index += 1;
      output += "\n";
      continue;
    }
    if (char === "/" && next === "*") {
      index += 2;
      while (index < text.length && !(text[index] === "*" && text[index + 1] === "/")) index += 1;
      index += 1;
      continue;
    }
    output += char;
  }
  return output;
}

function statMaybe(file) {
  try {
    return fs.statSync(file);
  } catch {
    return undefined;
  }
}
