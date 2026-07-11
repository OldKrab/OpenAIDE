import { createInterface } from "node:readline";
import type { ChildProcessWithoutNullStreams } from "node:child_process";
import { handleRuntimeLine, type RuntimeLineHandlerContext } from "./rpcLineHandler";

export function attachRuntimeLineReader(child: ChildProcessWithoutNullStreams, context: RuntimeLineHandlerContext) {
  const lines = createInterface({ input: child.stdout });
  lines.on("line", (line) => handleRuntimeLine(line, context));
}
