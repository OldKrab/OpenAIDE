import type { Dispatch } from "react";
import type { AppPreferencesRecord, HostToWebviewMessage } from "@openaide/app-shell-contracts";
import type { AppAction } from "./appReducer";
import type { PostHostMessage } from "./postHostMessage";
import type { WebviewBootstrap } from "./surfaceTypes";
import type { AgentOption } from "./composerOptions";

export type HostMessageRouterContext = {
  bootstrap: WebviewBootstrap;
  dispatch: Dispatch<AppAction>;
  openNewTaskSurface: (projectId?: string) => void;
  openSettingsSurface: () => void;
  setAgents: (agents: AgentOption[]) => void;
  setPreferences: (preferences: AppPreferencesRecord) => void;
  postHostMessage: PostHostMessage;
};

export type HostMessageRoute = (message: HostToWebviewMessage, context: HostMessageRouterContext) => boolean;
