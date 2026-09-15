// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { ProjectFilesButton } from "./ProjectFilesButton";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("moves the same accessible files action into the title row and back on resize", async () => {
  const container = document.createElement("div");
  const titleRow = document.createElement("header");
  document.body.append(titleRow, container);
  const root = createRoot(container);
  const open = vi.fn();
  try {
    await act(async () => root.render(<ProjectFilesButton open={false} onOpen={open} />));
    expect(container.querySelector("button")?.textContent).toBe("Project files");
    await act(async () => root.render(<ProjectFilesButton open={false} onOpen={open} target={titleRow} />));
    expect(container.querySelector("button")).toBeNull();
    const button = titleRow.querySelector("button")!;
    expect(button.getAttribute("aria-label")).toBe("Project files");
    expect(button.textContent).toBe("");
    expect(button.querySelector("svg")).not.toBeNull();
    await act(async () => button.click());
    expect(open).toHaveBeenCalledOnce();
    await act(async () => root.render(<ProjectFilesButton open={true} onOpen={open} target={titleRow} />));
    expect(titleRow.querySelector("button")?.getAttribute("aria-expanded")).toBe("true");
    await act(async () => root.render(<ProjectFilesButton open={true} onOpen={open} />));
    expect(titleRow.querySelector("button")).toBeNull();
    expect(container.querySelector("button")?.textContent).toBe("Project files");
  } finally {
    await act(async () => root.unmount());
    titleRow.remove();
    container.remove();
  }
});
