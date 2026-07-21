import type * as vscode from "vscode";
import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";
import type { ExtensionLogger } from "../logging/logger";
import type { RuntimeProcess } from "../runtime/process";
import type { RuntimeClient } from "../runtime/rpcClient";
import type { DeveloperSettingsStore } from "../settings/snapshot";

export type MessageContext = {
  runtime: RuntimeClient;
  runtimeProcess: RuntimeProcess;
  post: (payload: HostToWebviewMessage) => Thenable<boolean>;
  logger: ExtensionLogger;
  developerSettingsStore?: DeveloperSettingsStore;
  agentSecretStore?: vscode.SecretStorage;
  adoptTask?: (taskId: string, title?: string) => void;
  surfaces?: {
    openNewTask: (projectId?: string) => void;
    openNativeSession: (agentId: string, nativeSessionId: string, projectId?: string) => void;
    openSettings: (agentId?: string, returnToNewTask?: boolean, projectId?: string) => void;
    openTask: (taskId: string, title?: string) => void;
  };
};
