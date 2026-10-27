import { describe, expect, it, vi } from "vitest";
import { ExistingFileReferences } from "./existingFileReferences";

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("ExistingFileReferences", () => {
  it("asks about the paths of one tick together and remembers the answer", async () => {
    const check = vi.fn(async (paths: string[]) => paths.filter((path) => path.endsWith(".pdf")));
    const references = new ExistingFileReferences(check);
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
    const references = new ExistingFileReferences(check, () => now);

    references.request("out/report.md");
    await settle();
    now += 16_000;
    check.mockResolvedValueOnce(["out/report.md"]);
    references.request("out/report.md");
    await settle();

    expect(check).toHaveBeenCalledTimes(2);
    expect(references.state("out/report.md")).toBe("file");
  });

  it("leaves paths unlinked for a while after a failed check", async () => {
    let now = 1_000;
    const check = vi.fn(async (_paths: string[]) => {
      throw new Error("unsupported");
    });
    const references = new ExistingFileReferences(check, () => now);

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
