import type { McpServerSettingsRecord } from "@openaide/app-shell-contracts";
import { Globe2, Server, TerminalSquare } from "lucide-react";

export function McpTransportIcon({
  large = false,
  transport,
}: {
  large?: boolean;
  transport: McpServerSettingsRecord["transport"];
}) {
  const Icon = transport === "stdio" ? TerminalSquare : transport === "http" ? Globe2 : Server;
  return (
    <span className={`mcp-transport-icon ${large ? "large" : ""}`}>
      <Icon size={large ? 21 : 15} />
    </span>
  );
}

export function McpToggle({
  checked,
  label,
  onChange,
}: {
  checked: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      aria-label={label}
      aria-pressed={checked}
      className={`mcp-switch ${checked ? "checked" : ""}`}
      onClick={() => onChange(!checked)}
      type="button"
    >
      <span />
    </button>
  );
}
