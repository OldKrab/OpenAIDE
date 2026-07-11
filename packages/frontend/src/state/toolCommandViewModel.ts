export function cleanShellError(output: string) {
  return splitToolLines(output)
    .filter((line) => !/^```(?:\w+)?$/.test(line.trim()))
    .map((line) => line.replace(/^\w+:\d+:\s*/, "").trim())
    .filter(Boolean)
    .join("\n");
}

export function displayCommand(command: string[] | undefined) {
  const parts = command?.map((part) => part.trim()).filter(Boolean) ?? [];
  if (parts.length === 0) return undefined;
  if (parts.length >= 3 && isShellLauncher(parts[0]) && parts[1] === "-lc") return parts.slice(2).join(" ");
  return parts.join(" ");
}

export function splitToolLines(text: string) {
  const normalized = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  const lines = normalized.split("\n");
  return lines.length > 1 && lines[lines.length - 1] === "" ? lines.slice(0, -1) : lines;
}

function isShellLauncher(value: string) {
  return ["sh", "bash", "zsh"].includes(value.replace(/[\\/]+$/g, "").split(/[\\/]/).at(-1)?.toLowerCase() ?? value);
}
