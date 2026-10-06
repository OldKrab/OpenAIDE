import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/smoke",
  globalSetup: "./tests/smoke/global-setup.mjs",
  testMatch: ["session-deletion.spec.mjs", "config-preferences.spec.mjs", "task-chat.spec.mjs", "native-subagents.spec.mjs", "desktop-new-task.spec.mjs", "file-viewer-layout.spec.mjs", "web-secret-vault.spec.mjs"],
  fullyParallel: false,
  forbidOnly: true,
  retries: 0,
  // Watchdog for every polled assertion: it bounds a hung test and is far
  // longer than a loaded runner needs, so no assertion carries its own budget.
  expect: { timeout: 30_000 },
  reporter: [["list"], ["html", { open: "never", outputFolder: "test-results/report" }]],
  use: {
    browserName: "chromium",
    channel: "chrome",
    headless: true,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    video: "retain-on-failure",
  },
  outputDir: "test-results/artifacts",
});
