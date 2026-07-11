import type { HostToWebviewMessage } from "@openaide/app-shell-contracts";

export type HostMessageListener = (message: HostToWebviewMessage) => void;
export type UnsubscribeHostMessages = () => void;
export type SubscribeHostMessages = (listener: HostMessageListener) => UnsubscribeHostMessages;

export function startHostMessageSession(
  subscribe: SubscribeHostMessages,
  listener: HostMessageListener,
  start: () => void,
) {
  const unsubscribe = subscribe(listener);
  try {
    start();
  } catch (error) {
    unsubscribe();
    throw error;
  }
  return unsubscribe;
}
