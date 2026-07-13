import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/smoke",
  testMatch: "task-chat.spec.mjs",
  fullyParallel: false,
  forbidOnly: true,
  retries: 0,
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
