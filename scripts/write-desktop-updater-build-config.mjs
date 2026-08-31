import { writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/**
 * Materializes the updater identity for Tauri's bundler. The desktop reads the
 * same key from compile-time Rust configuration, which does not populate the
 * static Tauri configuration validated while updater artifacts are signed.
 */
export function writeDesktopUpdaterBuildConfig(outputPath, publicKey) {
  const normalizedKey = publicKey.trim();
  const decodedKey = Buffer.from(normalizedKey, "base64").toString("utf8");
  const [comment, key, ...extraLines] = decodedKey.trim().split(/\r?\n/);
  if (
    !normalizedKey ||
    Buffer.from(decodedKey, "utf8").toString("base64") !== normalizedKey ||
    !comment?.startsWith("untrusted comment: minisign public key:") ||
    !key?.startsWith("RW") ||
    extraLines.length > 0
  ) {
    throw new Error("Desktop updater release public key must decode to a minisign public key.");
  }
  writeFileSync(
    outputPath,
    `${JSON.stringify({ plugins: { updater: { pubkey: normalizedKey } } }, null, 2)}\n`,
  );
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : undefined;
if (invokedPath === fileURLToPath(import.meta.url)) {
  const [outputPath, publicKey] = process.argv.slice(2);
  if (!outputPath || !publicKey) {
    throw new Error(
      "Usage: node scripts/write-desktop-updater-build-config.mjs <output-path> <public-key>",
    );
  }
  writeDesktopUpdaterBuildConfig(outputPath, publicKey);
  console.log(`Wrote Tauri updater build configuration to ${outputPath}.`);
}
