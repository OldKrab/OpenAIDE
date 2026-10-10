import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const BINDINGS = "packages/app-server-client/src/generated/protocol.ts";
const VERSION_SOURCE = "openaide-rs/app-server-protocol/src/client.rs";
const RELEASED = "openaide-rs/app-server-protocol/released-protocol.json";
const VERSION_PATTERN = /APP_SERVER_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion \{\s*major: (\d+),\s*minor: (\d+),?\s*\}/;

/** Reads the `major.minor` App Server Protocol version declared in Rust. */
export function declaredVersion(source) {
  const match = VERSION_PATTERN.exec(source);
  if (!match) throw new Error(`APP_SERVER_PROTOCOL_VERSION was not found in ${VERSION_SOURCE}`);
  return { major: Number(match[1]), minor: Number(match[2]) };
}

export function bindingsHash(bindings) {
  return createHash("sha256").update(bindings).digest("hex");
}

/**
 * A Remote Device ships its own Frontend, so a protocol that differs from the
 * last release must say so in its version (ADR-0062). Returns the problem, or
 * `null` when the declared version accounts for the bindings.
 */
export function versionProblem({ released, current, hash }) {
  const [major, minor] = released.version.split(".").map(Number);
  if (hash === released.bindingsSha256) return null;
  if (current.major > major || (current.major === major && current.minor > minor)) return null;
  return [
    `The App Server Protocol differs from release ${released.version} but still declares ${current.major}.${current.minor}.`,
    `In ${VERSION_SOURCE}, bump minor for added methods, fields, or events,`,
    "or bump major and reset minor for a change an older client cannot ignore.",
  ].join("\n");
}

function main() {
  const current = declaredVersion(fs.readFileSync(path.join(root, VERSION_SOURCE), "utf8"));
  const hash = bindingsHash(fs.readFileSync(path.join(root, BINDINGS)));
  const releasedPath = path.join(root, RELEASED);
  if (process.argv.includes("--record")) {
    const released = { version: `${current.major}.${current.minor}`, bindingsSha256: hash };
    fs.writeFileSync(releasedPath, `${JSON.stringify(released, null, 2)}\n`);
    console.log(`recorded released app server protocol ${released.version}`);
    return;
  }
  const problem = versionProblem({ released: JSON.parse(fs.readFileSync(releasedPath, "utf8")), current, hash });
  if (problem) {
    console.error(problem);
    process.exit(1);
  }
  console.log("app server protocol version check passed");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
