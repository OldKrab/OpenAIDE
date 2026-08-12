import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const viteCli = fileURLToPath(new URL("../node_modules/vite/bin/vite.js", import.meta.url));
const forwardedArguments = process.argv.slice(2);

await runVite(["build", ...forwardedArguments]);
await runVite([
  "build",
  "--config",
  "vite.mermaid-renderer.config.ts",
  ...withoutEmptyOutDir(forwardedArguments),
  "--emptyOutDir=false",
]);

function withoutEmptyOutDir(arguments_) {
  return arguments_.filter((argument) => argument !== "--emptyOutDir" && !argument.startsWith("--emptyOutDir="));
}

function runVite(arguments_) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [viteCli, ...arguments_], { stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`Vite build failed (code=${code}, signal=${signal})`));
    });
  });
}
