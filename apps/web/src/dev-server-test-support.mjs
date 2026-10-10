/**
 * Resolves the port a spawned Web App Shell server bound, read from its listening
 * line. Spawn the shell with `OPENAIDE_WEB_PORT=0` so the OS assigns the port
 * to the shell itself and no other process can claim it first.
 */
export function listeningPort(child) {
  return new Promise((resolve, reject) => {
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
      const listening = / listening on http:\/\/[^\s:]+:(\d+)\r?\n/.exec(stdout);
      if (listening) resolve(Number(listening[1]));
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.once("error", reject);
    child.once("exit", (code) => reject(new Error(`Web server exited with ${code} before listening: ${stderr}`)));
  });
}
