import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const DOS_PE_POINTER_OFFSET = 0x3c;
const PE_SIGNATURE = "PE\0\0";
const COFF_HEADER_BYTES = 20;
const OPTIONAL_HEADER_SUBSYSTEM_OFFSET = 68;
const WINDOWS_GUI_SUBSYSTEM = 2;

/** Verifies that a PE image launches without Windows allocating a console. */
export async function assertWindowsGuiExecutable(executablePath) {
  const image = await readFile(executablePath);
  if (image.length < DOS_PE_POINTER_OFFSET + 4 || image.toString("ascii", 0, 2) !== "MZ") {
    throw new Error(`${executablePath} is not a DOS/PE executable`);
  }

  const peOffset = image.readUInt32LE(DOS_PE_POINTER_OFFSET);
  const optionalHeaderOffset = peOffset + PE_SIGNATURE.length + COFF_HEADER_BYTES;
  const subsystemOffset = optionalHeaderOffset + OPTIONAL_HEADER_SUBSYSTEM_OFFSET;
  if (
    subsystemOffset + 2 > image.length
    || image.toString("binary", peOffset, peOffset + PE_SIGNATURE.length) !== PE_SIGNATURE
  ) {
    throw new Error(`${executablePath} has an invalid PE header`);
  }

  const subsystem = image.readUInt16LE(subsystemOffset);
  if (subsystem !== WINDOWS_GUI_SUBSYSTEM) {
    throw new Error(
      `${executablePath} must use the Windows GUI subsystem; found PE subsystem ${subsystem}`,
    );
  }
}

async function main() {
  const [executablePath] = process.argv.slice(2);
  if (!executablePath) {
    throw new Error("Usage: node scripts/assert-windows-gui-executable.mjs <executable.exe>");
  }
  await assertWindowsGuiExecutable(path.resolve(executablePath));
  console.log(`Verified Windows GUI subsystem: ${executablePath}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
