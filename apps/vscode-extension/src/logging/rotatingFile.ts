import * as fs from "node:fs/promises";
import * as path from "node:path";

export type LogRotationPolicy = {
  maxBytes: number;
  /** Total files including the active log. */
  fileCount: number;
};

export const DEFAULT_LOG_ROTATION: LogRotationPolicy = {
  maxBytes: 32 * 1024 * 1024,
  fileCount: 4,
};

/** Serializes bounded writes for one process-owned diagnostics file. */
export class RotatingLogFile {
  private pending: Promise<void> = Promise.resolve();
  private currentBytes: number | undefined;

  constructor(
    private readonly filePath: string,
    private readonly policy: LogRotationPolicy = DEFAULT_LOG_ROTATION,
  ) {}

  append(line: string): Promise<void> {
    const operation = this.pending.then(() => this.appendNow(line));
    // A failed write must not permanently poison later diagnostic writes.
    this.pending = operation.catch(() => undefined);
    return operation;
  }

  private async appendNow(line: string) {
    await fs.mkdir(path.dirname(this.filePath), { recursive: true });
    const bytes = Buffer.byteLength(`${line}\n`, "utf8");
    if (bytes > this.policy.maxBytes) {
      throw new RangeError("log record exceeds the rotation file limit");
    }
    if (this.currentBytes === undefined) {
      this.currentBytes = await fileSize(this.filePath);
      if (this.currentBytes >= this.policy.maxBytes && this.currentBytes > 0) {
        await rotateFiles(this.filePath, this.policy.fileCount);
        this.currentBytes = 0;
      }
    }
    if (this.currentBytes > 0 && this.currentBytes + bytes > this.policy.maxBytes) {
      await rotateFiles(this.filePath, this.policy.fileCount);
      this.currentBytes = 0;
    }
    await fs.appendFile(this.filePath, `${line}\n`, "utf8");
    this.currentBytes += bytes;
  }
}

async function rotateFiles(filePath: string, fileCount: number) {
  if (fileCount <= 1) {
    await removeIfPresent(filePath);
    return;
  }
  for (let generation = fileCount - 1; generation >= 1; generation -= 1) {
    const source = generation === 1 ? filePath : backupPath(filePath, generation - 1);
    const destination = backupPath(filePath, generation);
    await removeIfPresent(destination);
    try {
      await fs.rename(source, destination);
    } catch (error) {
      if (!isNotFound(error)) throw error;
    }
  }
}

function backupPath(filePath: string, generation: number) {
  return `${filePath}.${generation}`;
}

async function fileSize(filePath: string) {
  try {
    return (await fs.stat(filePath)).size;
  } catch (error) {
    if (isNotFound(error)) return 0;
    throw error;
  }
}

async function removeIfPresent(filePath: string) {
  try {
    await fs.unlink(filePath);
  } catch (error) {
    if (!isNotFound(error)) throw error;
  }
}

function isNotFound(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error && error.code === "ENOENT";
}
