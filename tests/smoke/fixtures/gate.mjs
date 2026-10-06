import { existsSync } from "node:fs";
import path from "node:path";

/**
 * Holds a fixture while the test keeps the named gate closed. Tests close and
 * open gates through the harness, so fixture ordering never depends on a delay.
 */
export async function released(gate) {
  const file = path.join(process.env.OPENAIDE_SMOKE_GATE_ROOT, gate);
  while (existsSync(file)) {
    await new Promise((resolve) => setTimeout(resolve, 10)); // timing: poll
  }
}
