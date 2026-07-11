const { test, expect } = require("@playwright/test");

test("collapsed project remains collapsed after reload", async ({ page }) => {
  await page.goto("http://127.0.0.1:5574");

  const project = page.getByRole("button", { name: /^old 1 task$/ });
  await expect(project).toHaveAttribute("aria-expanded", "true");
  await project.click();
  await expect(project).toHaveAttribute("aria-expanded", "false");

  await page.reload();
  await expect(project).toHaveAttribute("aria-expanded", "false");
});
