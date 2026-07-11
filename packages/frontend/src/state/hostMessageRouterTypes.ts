import type { Dispatch } from "react";
import type { AppPreferencesRecord, HostToWebviewMessage } from "@openaide/app-shell-contracts";
import type { SnapshotIntent } from "./appReducer";
import type { AppAction } from "./appReducer";
import type { SnapshotRequestTracker } from "./snapshotRequests";
import type { PostHostMessage } from "./postHostMessage";
import type { WebviewBootstrap } from "./surfaceTypes";
import type { AgentOption } from "./composerOptions";

export type MutableRef<T> = { current: T };

export type HostMessageRouterContext = {
  bootstrap: WebviewBootstrap;
  dispatch: Dispatch<AppAction>;
  setAgents: (agents: AgentOption[]) => void;
  setPreferences: (preferences: AppPreferencesRecord) => void;
  snapshotRequests: MutableRef<SnapshotRequestTracker>;
  latestOptionsRequestKey: MutableRef<string | undefined>;
  latestSessionListRequestId: MutableRef<number | undefined>;
  nextSessionListRequestId: MutableRef<number>;
  latestNativeSessionSelection: MutableRef<{ agentId: string; workspaceRoot: string }>;
  createSnapshotRequestId: (taskId?: string, intent?: SnapshotIntent) => number;
  postHostMessage: PostHostMessage;
};

export type HostMessageRoute = (message: HostToWebviewMessage, context: HostMessageRouterContext) => boolean;
