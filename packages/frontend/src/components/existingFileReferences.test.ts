import { describe, expect, it, vi } from "vitest";
import { ExistingFileReferences } from "./existingFileReferences";

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const drainNow = (drain: () => void) => queueMicrotask(drain);

describe("ExistingFileReferences", () => {
  it("asks about the paths of one tick together and remembers the answer", async () => {
    const check = vi.fn(async (paths: string[]) => paths.filter((path) => path.endsWith(".pdf")));
    const references = new ExistingFileReferences(check, Date.now, drainNow);
    const changed = vi.fn();
    references.subscribe(changed);

    references.request("cv/resume.pdf");
    references.request("tool.openPath");
    references.request("cv/resume.pdf");
    expect(references.state("cv/resume.pdf")).toBe("pending");
    await settle();

    expect(check).toHaveBeenCalledOnce();
    expect(check).toHaveBeenCalledWith(["cv/resume.pdf", "tool.openPath"]);
    expect(references.state("cv/resume.pdf")).toBe("file");
    expect(references.state("tool.openPath")).toBe("missing");
    expect(changed).toHaveBeenCalled();

    references.request("cv/resume.pdf");
    references.request("tool.openPath");
    await settle();
    expect(check).toHaveBeenCalledOnce();
  });

  it("asks about a missing path again later, since the Agent may create it", async () => {
    let now = 1_000;
    const check = vi.fn(async (_paths: string[]) => [] as string[]);
    const references = new ExistingFileReferences(check, () => now, drainNow);

    references.request("out/report.md");
    await settle();
    now += 16_000;
    check.mockResolvedValueOnce(["out/report.md"]);
    references.request("out/report.md");
    await settle();

    expect(check).toHaveBeenCalledTimes(2);
    expect(references.state("out/report.md")).toBe("file");
  });

  it("lets paths from later ticks share one request until the window closes", async () => {
    const check = vi.fn(async (_paths: string[]) => [] as string[]);
    let closeWindow: (() => void) | undefined;
    const references = new ExistingFileReferences(check, Date.now, (drain) => {
      closeWindow = drain;
    });

    references.request("row/one.md");
    await settle();
    references.request("row/two.md");
    await settle();
    expect(check).not.toHaveBeenCalled();

    closeWindow?.();
    await settle();

    expect(check).toHaveBeenCalledOnce();
    expect(check).toHaveBeenCalledWith(["row/one.md", "row/two.md"]);
  });

  it("sends one request at a time and groups what arrives meanwhile", async () => {
    const answers: ((files: string[]) => void)[] = [];
    const check = vi.fn((_paths: string[]) => new Promise<string[]>((resolve) => answers.push(resolve)));
    const references = new ExistingFileReferences(check, Date.now, drainNow);

    references.request("a.md");
    await settle();
    references.request("b.md");
    references.request("c.md");
    await settle();
    expect(check).toHaveBeenCalledOnce();

    answers[0]([]);
    await settle();

    expect(check).toHaveBeenCalledTimes(2);
    expect(check).toHaveBeenLastCalledWith(["b.md", "c.md"]);
    answers[1](["c.md"]);
    await settle();
    expect(references.state("c.md")).toBe("file");

    references.request("d.md");
    await settle();
    expect(check).toHaveBeenCalledTimes(3);
  });

  it("drops queued paths instead of retrying through a failure", async () => {
    let reject: ((error: Error) => void) | undefined;
    const check = vi.fn((_paths: string[]) => new Promise<string[]>((_resolve, fail) => {
      reject = fail;
    }));
    const references = new ExistingFileReferences(check, () => 1_000, drainNow);

    references.request("a.md");
    await settle();
    references.request("b.md");
    reject?.(new Error("unsupported"));
    await settle();

    expect(check).toHaveBeenCalledOnce();
    expect(references.state("b.md")).toBe("missing");
  });

  it("leaves paths unlinked for a while after a failed check", async () => {
    let now = 1_000;
    const check = vi.fn(async (_paths: string[]) => {
      throw new Error("unsupported");
    });
    const references = new ExistingFileReferences(check, () => now, drainNow);

    references.request("a.md");
    await settle();
    references.request("b.md");
    await settle();

    expect(check).toHaveBeenCalledOnce();
    expect(references.state("b.md")).toBe("missing");
    now += 61_000;
    expect(references.state("b.md")).toBe("pending");
  });
});
