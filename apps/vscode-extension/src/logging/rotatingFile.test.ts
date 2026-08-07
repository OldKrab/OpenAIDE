import * as fs from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { RotatingLogFile } from "./rotatingFile";

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((directory) => fs.rm(directory, { recursive: true })));
});

describe("RotatingLogFile", () => {
  it("keeps only the configured complete generations", async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), "openaide-log-rotation-"));
    temporaryDirectories.push(directory);
    const filePath = path.join(directory, "extension.jsonl");
    const log = new RotatingLogFile(filePath, { maxBytes: 10, fileCount: 4 });

    for (let index = 0; index < 10; index += 1) {
      await log.append(String(index).padStart(4, "0"));
    }

    await expect(fs.readFile(filePath, "utf8")).resolves.toBe("0008\n0009\n");
    await expect(fs.readFile(`${filePath}.1`, "utf8")).resolves.toBe("0006\n0007\n");
    await expect(fs.readFile(`${filePath}.2`, "utf8")).resolves.toBe("0004\n0005\n");
    await expect(fs.readFile(`${filePath}.3`, "utf8")).resolves.toBe("0002\n0003\n");
    await expect(fs.stat(`${filePath}.4`)).rejects.toMatchObject({ code: "ENOENT" });
  });

  it("moves an existing oversized file before appending", async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), "openaide-log-rotation-"));
    temporaryDirectories.push(directory);
    const filePath = path.join(directory, "extension.jsonl");
    await fs.writeFile(filePath, "legacy-record\n");
    const log = new RotatingLogFile(filePath, { maxBytes: 8, fileCount: 4 });

    await log.append("new");

    await expect(fs.readFile(filePath, "utf8")).resolves.toBe("new\n");
    await expect(fs.readFile(`${filePath}.1`, "utf8")).resolves.toBe("legacy-record\n");
  });

  it("rejects one record larger than the file limit", async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), "openaide-log-rotation-"));
    temporaryDirectories.push(directory);
    const filePath = path.join(directory, "extension.jsonl");
    const log = new RotatingLogFile(filePath, { maxBytes: 8, fileCount: 4 });

    await expect(log.append("record-too-large")).rejects.toThrow(RangeError);
    await expect(fs.stat(filePath)).rejects.toMatchObject({ code: "ENOENT" });
  });
});
