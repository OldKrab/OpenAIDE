import { createContext, useContext } from "react";

export type OpenAgentFileReference = (path: string, line?: number) => void;

export const AgentFileOpenContext = createContext<OpenAgentFileReference | undefined>(undefined);

export function useAgentFileOpen(): OpenAgentFileReference | undefined {
  return useContext(AgentFileOpenContext);
}
