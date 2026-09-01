import { expect, it, vi } from "vitest";
import { createDesktopSupportExports } from "./desktopSupportExports";

it("saves through the native Desktop boundary", async () => {
  const invoke = vi.fn(async () => true);
  const supportExports = createDesktopSupportExports(invoke);

  const outcome = await supportExports.save({
    fileHandleId: "export-1",
    label: "openaide-support-123.zip",
  });

  expect(outcome).toBe("saved");
  expect(invoke).toHaveBeenCalledWith("desktop_save_support_export", {
    fileHandleId: "export-1",
    label: "openaide-support-123.zip",
  });
});

it("keeps the export dialog open when the native save dialog is canceled", async () => {
  const supportExports = createDesktopSupportExports(vi.fn(async () => false));

  await expect(supportExports.save({ fileHandleId: "export-1", label: "support.zip" }))
    .resolves.toBe("cancelled");
});
