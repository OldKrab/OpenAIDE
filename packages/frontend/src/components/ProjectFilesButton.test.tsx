// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { ProjectFilesButton } from "./ProjectFilesButton";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("shows the change count in the Task header and as a badge in a compact header", async () => {
  const container = document.createElement("div");
  const titleRow = document.createElement("header");
  document.body.append(titleRow, container);
  const root = createRoot(container);
  const toggle = vi.fn();
  try {
    await act(async () => root.render(<ProjectFilesButton open={false} onToggle={toggle} />));
    expect(container.querySelector("button")?.textContent).toBe("Files");
    expect(container.querySelector("button")?.getAttribute("aria-label")).toBe("Project files");

    await act(async () => root.render(<ProjectFilesButton added={12} count={3} open={false} onToggle={toggle} removed={4} />));
    expect(container.querySelector("button")?.textContent).toBe("3 files+12−4");
    expect(container.querySelector("button")?.getAttribute("aria-label")).toBe(
      "Project files, 3 changed files, 12 added, 4 removed",
    );

    await act(async () =>
      root.render(<ProjectFilesButton added={12} count={3} open={false} onToggle={toggle} removed={4} target={titleRow} />),
    );
    expect(container.querySelector("button")).toBeNull();
    const button = titleRow.querySelector("button")!;
    expect(button.textContent).toBe("3");
    expect(button.getAttribute("aria-label")).toContain("3 changed files");
    await act(async () => button.click());
    expect(toggle).toHaveBeenCalledOnce();

    await act(async () => root.render(<ProjectFilesButton open={true} onToggle={toggle} target={titleRow} />));
    expect(titleRow.querySelector("button")?.getAttribute("aria-expanded")).toBe("true");
    expect(titleRow.querySelector("button")?.textContent).toBe("");
    await act(async () => root.render(<ProjectFilesButton open={true} onToggle={toggle} />));
    expect(titleRow.querySelector("button")).toBeNull();
  } finally {
    await act(async () => root.unmount());
    titleRow.remove();
    container.remove();
  }
});
