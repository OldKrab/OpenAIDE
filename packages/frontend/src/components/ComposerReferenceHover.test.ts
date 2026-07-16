import { describe, expect, it } from "vitest";
import {
  referenceHoverModelFromElement,
  referenceHoverPosition,
} from "./ComposerReferenceHover";

describe("composer reference hover", () => {
  it("anchors to each token and clamps or flips at viewport edges", () => {
    const popup = { height: 70, width: 300 };
    const viewport = { height: 600, width: 800 };

    expect(referenceHoverPosition(
      { bottom: 120, left: 40, top: 100 },
      popup,
      viewport,
    )).toEqual({ left: 40, top: 128 });
    expect(referenceHoverPosition(
      { bottom: 120, left: 300, top: 100 },
      popup,
      viewport,
    )).toEqual({ left: 300, top: 128 });
    expect(referenceHoverPosition(
      { bottom: 720, left: 430, top: 700 },
      { height: 180, width: 390 },
      { height: 760, width: 480 },
    )).toEqual({ left: 78, top: 512 });
  });

  it("reads only complete typed metadata from a rendered reference", () => {
    expect(referenceHoverModelFromElement({
      dataset: {
        referenceDescription: "Markdown · Workspace root",
        referenceKind: "file",
        referenceLabel: "AGENTS.md",
        referenceType: "Workspace file",
      },
    } as unknown as HTMLElement)).toEqual({
      description: "Markdown · Workspace root",
      kind: "file",
      label: "AGENTS.md",
      type: "Workspace file",
    });
    expect(referenceHoverModelFromElement({
      dataset: { referenceKind: "command", referenceLabel: "/review" },
    } as unknown as HTMLElement)).toBeUndefined();
  });
});
