import { expect, test } from "@playwright/test";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { startFullStackHarness } from "./full-stack-harness.mjs";

let harness;
test.setTimeout(180_000);
test.beforeAll(async () => {
  harness = await startFullStackHarness({ agentFixture: fileURLToPath(new URL("./fixtures/preferences-acp-agent.mjs", import.meta.url)) });
});
test.afterEach(async ({}, info) => {
  if (info.status === info.expectedStatus || !harness) return;
  const files = await readdir(harness.stateRoot, { recursive: true });
  const diagnostics = [];
  for (const file of files.filter((file) => file.endsWith(".jsonl"))) {
    const content = await readFile(path.join(harness.stateRoot, file), "utf8");
    diagnostics.push(...content.split("\n").filter((line) => line.includes("preference")));
  }
  await info.attach("preference-diagnostics", { body: diagnostics.join("\n"), contentType: "application/x-ndjson" });
});
test.afterAll(async () => { await harness?.close(); });

test("shows live preferences and explicit recovery at wide and narrow widths", async ({ page }, info) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto(`${harness.baseUrl}/new-task`);
  await expect(page.getByRole("button", { name: "Balanced", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Balanced", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Balanced", exact: true }).click();
  await page.getByRole("menu", { name: "Test mode" }).getByRole("menuitemradio", { name: "Verbose" }).click();
  await expect(page.getByRole("button", { name: "Verbose", exact: true })).toBeVisible();
  await page.getByRole("textbox", { name: "Message", exact: true }).fill("smoke:basic");
  await page.getByLabel("Send message", { exact: true }).click();
  await expect(page.getByLabel("Task chat", { exact: true })).toBeVisible();

  for (const [width, action] of [[1280, "Retry"], [390, "Use current settings"]]) {
    await page.setViewportSize({ width, height: 800 });
    await page.goto(`${harness.baseUrl}/new-task`);
    await expect(page.getByText("Connecting to OpenAIDE Test Agent…", { exact: true })).toBeVisible();
    const editor = page.getByRole("textbox", { name: "Message", exact: true });
    await editor.fill("smoke:basic");
    await expect(page.getByText("Applying your preferences…", { exact: true })).toBeVisible();
    await expect(page.getByLabel("Balanced → Verbose, updating Agent option", { exact: true }).filter({ visible: true })).toHaveAttribute("aria-disabled", "true");
    await expect(page.getByLabel("Send message", { exact: true })).toBeDisabled();
    await expect(editor).toBeEditable();
    await page.screenshot({ path: info.outputPath(`preferences-${width}-applying.png`) });
    await expect(page.getByText("Couldn’t apply your preferences.", { exact: true })).toBeVisible();
    await expect(page.getByLabel("Send message", { exact: true })).toBeDisabled();
    await page.screenshot({ path: info.outputPath(`preferences-${width}-failed.png`) });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
    await page.getByRole("button", { name: action, exact: true }).click();
    await expect(page.getByRole("button", { name: action === "Retry" ? "Verbose" : "Balanced", exact: true })).toBeEnabled();
    await expect(page.getByLabel("Send message", { exact: true })).toBeEnabled();
    await page.getByLabel("Send message", { exact: true }).click();
    await expect(page.getByLabel("Task chat", { exact: true })).toBeVisible();
  }
});
