import { beforeEach, describe, expect, it, vi } from "vitest";
import { readTextFile, registerFileSystemHostHandlers, writeTextFile } from "./hostFileSystem";

const fsMocks = vi.hoisted(() => ({
  realpath: vi.fn(async (path: string) => path),
}));

const vscodeMocks = vi.hoisted(() => ({
  applyEdit: vi.fn(),
  createDirectory: vi.fn(),
  openTextDocument: vi.fn(),
  stat: vi.fn(),
  writeFile: vi.fn(),
  workspaceFolders: [{ uri: { fsPath: "/workspace/app" }, name: "App" }],
}));

vi.mock("node:fs/promises", () => ({
  realpath: fsMocks.realpath,
}));

vi.mock("vscode", () => ({
  Position: class Position {
    constructor(
      readonly line: number,
      readonly character: number,
    ) {}
  },
  Range: class Range {
    constructor(
      readonly start: unknown,
      readonly end: unknown,
    ) {}
  },
  Uri: {
    file: (fsPath: string) => ({ fsPath }),
  },
  WorkspaceEdit: class WorkspaceEdit {
    replacements: unknown[] = [];

    replace(uri: unknown, range: unknown, content: string) {
      this.replacements.push({ uri, range, content });
    }
  },
  workspace: {
    get workspaceFolders() {
      return vscodeMocks.workspaceFolders;
    },
    applyEdit: vscodeMocks.applyEdit,
    fs: {
      createDirectory: vscodeMocks.createDirectory,
      stat: vscodeMocks.stat,
      writeFile: vscodeMocks.writeFile,
    },
    openTextDocument: vscodeMocks.openTextDocument,
  },
}));

describe("ACP host filesystem handlers", () => {
  beforeEach(() => {
    vscodeMocks.applyEdit.mockReset().mockResolvedValue(true);
    vscodeMocks.createDirectory.mockReset().mockResolvedValue(undefined);
    vscodeMocks.openTextDocument.mockReset();
    vscodeMocks.stat.mockReset().mockResolvedValue({ type: "file" });
    vscodeMocks.writeFile.mockReset().mockResolvedValue(undefined);
    vscodeMocks.workspaceFolders = [{ uri: { fsPath: "/workspace/app" }, name: "App" }];
    fsMocks.realpath.mockReset().mockImplementation(async (filePath: string) => {
      if (filePath === "/missing" || filePath.startsWith("/missing/")) {
        const error = new Error("missing") as NodeJS.ErrnoException;
        error.code = "ENOENT";
        throw error;
      }
      return filePath;
    });
  });

  it("registers read and write handlers on the runtime client", () => {
    const disposables = [disposable(), disposable()];
    const runtime = {
      onHostRequest: vi.fn()
        .mockReturnValueOnce(disposables[0])
        .mockReturnValueOnce(disposables[1]),
    };

    const registered = registerFileSystemHostHandlers(runtime as never);

    expect(runtime.onHostRequest).toHaveBeenNthCalledWith(1, "fs/read_text_file", expect.any(Function));
    expect(runtime.onHostRequest).toHaveBeenNthCalledWith(2, "fs/write_text_file", expect.any(Function));

    registered.dispose();
    expect(disposables[0].dispose).toHaveBeenCalled();
    expect(disposables[1].dispose).toHaveBeenCalled();
  });

  it("reads open document text with one-based line and limit support", async () => {
    vscodeMocks.openTextDocument.mockResolvedValue(document(["zero\n", "one\n", "two\n"]));

    const result = await readTextFile({
      sessionId: "session_1",
      path: "/workspace/app/src/main.rs",
      line: 2,
      limit: 1,
    });

    expect(vscodeMocks.openTextDocument).toHaveBeenCalledWith({ fsPath: "/workspace/app/src/main.rs" });
    expect(result).toEqual({ content: "one\n" });
  });

  it("rejects reads outside the current workspace", async () => {
    await expect(readTextFile({ path: "/tmp/secret.txt" })).rejects.toThrow("outside the current workspace");
    expect(vscodeMocks.openTextDocument).not.toHaveBeenCalled();
  });

  it("rejects symlink escapes after resolving real paths", async () => {
    fsMocks.realpath.mockImplementation(async (filePath: string) => {
      if (filePath === "/workspace/app") return "/workspace/app";
      if (filePath === "/workspace/app/link/secret.txt") return "/home/user/secret.txt";
      return filePath;
    });

    await expect(readTextFile({ path: "/workspace/app/link/secret.txt" })).rejects.toThrow(
      "outside the current workspace",
    );
    expect(vscodeMocks.openTextDocument).not.toHaveBeenCalled();
  });

  it("writes through a workspace edit after creating missing files", async () => {
    const missing = new Error("missing") as NodeJS.ErrnoException;
    missing.code = "ENOENT";
    vscodeMocks.stat.mockRejectedValueOnce(missing);
    fsMocks.realpath.mockImplementation(async (filePath: string) => {
      if (filePath === "/workspace/app/src/new.ts") {
        const error = new Error("missing") as NodeJS.ErrnoException;
        error.code = "ENOENT";
        throw error;
      }
      return filePath;
    });
    const openedDocument = document(["old\n"]);
    vscodeMocks.openTextDocument.mockResolvedValueOnce(openedDocument);

    await writeTextFile({
      sessionId: "session_1",
      path: "/workspace/app/src/new.ts",
      content: "next\n",
    });

    expect(vscodeMocks.createDirectory).toHaveBeenCalledWith({ fsPath: "/workspace/app/src" });
    expect(vscodeMocks.writeFile).toHaveBeenCalledWith({ fsPath: "/workspace/app/src/new.ts" }, new Uint8Array());
    expect(vscodeMocks.applyEdit).toHaveBeenCalledTimes(1);
    const edit = vscodeMocks.applyEdit.mock.calls[0]?.[0] as { replacements: Array<{ content: string }> };
    expect(edit.replacements[0].content).toBe("next\n");
    expect(openedDocument.save).toHaveBeenCalledTimes(1);
  });

  it("rejects writes when the editor buffer has unsaved changes", async () => {
    const save = vi.fn();
    vscodeMocks.openTextDocument.mockResolvedValueOnce({
      ...document(["unsaved user work\n"]),
      isDirty: true,
      save,
      version: 4,
    });

    await expect(
      writeTextFile({
        path: "/workspace/app/src/main.ts",
        content: "agent replacement\n",
      }),
    ).rejects.toThrow("unsaved editor changes");

    expect(vscodeMocks.applyEdit).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();
  });

  it("rejects writes when the document changes before the workspace edit is applied", async () => {
    const save = vi.fn();
    let version = 7;
    const changingDocument = {
      ...document(["original\n"]),
      get version() {
        const current = version;
        version += 1;
        return current;
      },
      isDirty: false,
      save,
    };
    vscodeMocks.openTextDocument.mockResolvedValueOnce(changingDocument);

    await expect(
      writeTextFile({
        path: "/workspace/app/src/main.ts",
        content: "agent replacement\n",
      }),
    ).rejects.toThrow("changed before the Agent edit could be applied");

    expect(vscodeMocks.applyEdit).not.toHaveBeenCalled();
    expect(save).not.toHaveBeenCalled();
  });

  it("rejects writes when the edited document cannot be saved", async () => {
    const save = vi.fn(async () => false);
    vscodeMocks.openTextDocument.mockResolvedValueOnce({
      ...document(["original\n"]),
      isDirty: false,
      save,
      version: 2,
    });

    await expect(
      writeTextFile({
        path: "/workspace/app/src/main.ts",
        content: "agent replacement\n",
      }),
    ).rejects.toThrow("Unable to save file edit");

    expect(vscodeMocks.applyEdit).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("does not report success when the document changes while it is being saved", async () => {
    const openedDocument = {
      ...document(["original\n"]),
      isDirty: false,
      version: 2,
    };
    openedDocument.save.mockImplementationOnce(async () => {
      openedDocument.isDirty = true;
      return true;
    });
    vscodeMocks.openTextDocument.mockResolvedValueOnce(openedDocument);

    await expect(
      writeTextFile({
        path: "/workspace/app/src/main.ts",
        content: "agent replacement\n",
      }),
    ).rejects.toThrow("File changed while the Agent edit was being saved");

    expect(vscodeMocks.applyEdit).toHaveBeenCalledTimes(1);
    expect(openedDocument.save).toHaveBeenCalledTimes(1);
  });

  it("does not create an empty file when an existing document cannot open", async () => {
    vscodeMocks.openTextDocument.mockRejectedValueOnce(new Error("unsupported encoding"));

    await expect(
      writeTextFile({
        path: "/workspace/app/src/binary.bin",
        content: "next\n",
      }),
    ).rejects.toThrow("unsupported encoding");

    expect(vscodeMocks.writeFile).not.toHaveBeenCalled();
    expect(vscodeMocks.applyEdit).not.toHaveBeenCalled();
  });

  it("does not create files after non-missing stat failures", async () => {
    const denied = new Error("denied") as NodeJS.ErrnoException;
    denied.code = "EACCES";
    vscodeMocks.stat.mockRejectedValueOnce(denied);

    await expect(
      writeTextFile({
        path: "/workspace/app/src/locked.ts",
        content: "next\n",
      }),
    ).rejects.toThrow("denied");

    expect(vscodeMocks.writeFile).not.toHaveBeenCalled();
    expect(vscodeMocks.createDirectory).not.toHaveBeenCalled();
    expect(vscodeMocks.applyEdit).not.toHaveBeenCalled();
  });
});

function disposable() {
  return { dispose: vi.fn() };
}

function document(lines: string[]) {
  const content = lines.join("");
  return {
    isDirty: false,
    lineCount: lines.length,
    save: vi.fn(async () => true),
    version: 1,
    getText: (range?: { start: { line: number }; end: { line: number } }) => {
      if (!range) return content;
      return lines.slice(range.start.line, range.end.line).join("");
    },
    lineAt: (line: number) => ({
      rangeIncludingLineBreak: {
        end: { line: line + 1, character: 0 },
      },
    }),
  };
}
