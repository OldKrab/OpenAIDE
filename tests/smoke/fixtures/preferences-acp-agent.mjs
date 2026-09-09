import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

// Exercise the real ACP stream while making startup and preference writes observable.
const agent = spawn(process.execPath, [fileURLToPath(new URL("./test-acp-agent.mjs", import.meta.url))], { stdio: ["pipe", "pipe", "inherit"] });
agent.stdout.pipe(process.stdout);
let writes = 0;
createInterface({ input: process.stdin }).on("line", (line) => {
  const message = JSON.parse(line);
  if (message.method === "session/set_config_option") {
    writes += 1;
    if (writes % 2 === 0) {
      setTimeout(() => process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: message.id, error: { code: -32603, message: "Simulated preference failure" } })}\n`), 4_000);
      return;
    }
  }
  setTimeout(() => agent.stdin.write(`${line}\n`), message.method === "session/new" ? 1_000 : 0);
});
process.stdin.on("end", () => agent.kill());
process.on("SIGTERM", () => { agent.kill(); process.exit(0); });
agent.on("exit", (code) => process.exit(code ?? 0));
