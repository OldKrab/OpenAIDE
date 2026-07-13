import type { WebviewToHostMessage } from "@openaide/app-shell-contracts";

export type SurfaceNavigationMessage = Extract<
  WebviewToHostMessage,
  { type: "surface.openArchive" | "surface.openNewTask" | "surface.openSettings" | "surface.openTask" }
>;

export type HostChannelMessage = Exclude<WebviewToHostMessage, SurfaceNavigationMessage>;
export type PostHostMessage = (message: HostChannelMessage) => void;
