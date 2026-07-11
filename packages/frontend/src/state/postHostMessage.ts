import type { WebviewToHostMessage } from "@openaide/app-shell-contracts";

export type PostHostMessage = (message: WebviewToHostMessage) => void;
