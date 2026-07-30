import * as vscode from "vscode";
import {
  agentCatalogEntry,
  normalizedAgentIcon,
  type AgentIconId,
} from "@openaide/app-shell-contracts";

type TabIconId = AgentIconId | "new-task" | "settings";

export function newTaskTabIcon(context: vscode.ExtensionContext) {
  return tabIcon(context, "new-task");
}

export function settingsTabIcon(context: vscode.ExtensionContext) {
  return tabIcon(context, "settings");
}

export function agentTabIcon(context: vscode.ExtensionContext, agentId?: string) {
  return tabIcon(context, agentIcon(agentId));
}

function tabIcon(context: vscode.ExtensionContext, icon: TabIconId) {
  return vscode.Uri.joinPath(context.extensionUri, "media", "tab-icons", `${icon}.svg`);
}

/** Resolves the same configured Agent icon used by the Frontend, with a safe generic fallback. */
function agentIcon(agentId?: string): AgentIconId {
  if (!agentId) return "bot";
  const builtIn = agentCatalogEntry(agentId);
  if (builtIn) return builtIn.icon;
  const configured = vscode.workspace.getConfiguration("openaide").get<unknown>("agents");
  if (!Array.isArray(configured)) return "bot";
  const record = configured.find((candidate) => (
    typeof candidate === "object"
    && candidate !== null
    && "id" in candidate
    && candidate.id === agentId
  ));
  return normalizedAgentIcon(
    typeof record === "object" && record !== null && "icon" in record ? record.icon : undefined,
  ) ?? "bot";
}
