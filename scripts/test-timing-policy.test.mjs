// timing-file: data — rule samples held as strings.
import assert from "node:assert/strict";
import test from "node:test";

import { isTestSource, timingViolations } from "./test-timing-policy.mjs";

const reasons = (file, source) => timingViolations(file, source).map(({ reason }) => reason);

test("covers test code and leaves production source and the vocabulary alone", () => {
  assert.equal(isTestSource("openaide-rs/app-server/src/tasks/turns_tests.rs"), true);
  assert.equal(isTestSource("openaide-rs/app-server/tests/runtime_contract/support.rs"), true);
  assert.equal(isTestSource("tests/smoke/fixtures/test-acp-agent.mjs"), true);
  assert.equal(isTestSource("packages/frontend/src/components/App.test.tsx"), true);
  assert.equal(isTestSource("openaide-rs/app-server/src/test_sync.rs"), false);
  assert.equal(isTestSource("openaide-rs/app-server/src/tasks/turns.rs"), false);
  assert.equal(isTestSource("packages/frontend/node_modules/pkg/index.test.js"), false);
});

test("reports Rust tests that sleep, size a duration, or assert elapsed time", () => {
  assert.deepEqual(reasons("a_tests.rs", "std::thread::sleep(pause);"), ["sleeps"]);
  assert.deepEqual(reasons("a_tests.rs", "rx.recv_timeout(Duration::from_secs(2))"), ["sizes a duration"]);
  assert.deepEqual(reasons("a_tests.rs", "assert!(started.elapsed() < limit);"), ["asserts elapsed time"]);
  assert.deepEqual(reasons("a_tests.rs", "    time.sleep(0.01)"), ["sleeps"]);
});

test("reports script tests that sleep or run on a private budget", () => {
  assert.deepEqual(reasons("a.spec.mjs", "await page.waitForTimeout(400);"), ["sleeps"]);
  assert.deepEqual(reasons("a.test.mjs", "await new Promise((resolve) => setTimeout(resolve, 50));"), [
    "sleeps, sizes a duration below the watchdog",
  ]);
  assert.deepEqual(reasons("a.spec.mjs", "await expect(row).toBeVisible({ timeout: 5_000 });"), [
    "sizes a duration below the watchdog",
  ]);
  assert.deepEqual(reasons("a.test.mjs", "const deadline = Date.now() + 2_000;"), [
    "sizes a duration below the watchdog",
  ]);
  assert.deepEqual(reasons("a.test.mjs", "if (Date.now() - started > limit) fail();"), ["asserts elapsed time"]);
});

test("accepts the shared watchdog, yields, and ordinary numbers", () => {
  assert.deepEqual(reasons("a_tests.rs", "rx.recv_timeout(WATCHDOG)"), []);
  assert.deepEqual(reasons("a_tests.rs", "const WATCHDOG: Duration = Duration::from_secs(30);"), []);
  assert.deepEqual(reasons("a.test.mjs", "test.setTimeout(180_000);"), []);
  assert.deepEqual(reasons("a.test.mjs", "const timer = setTimeout(fail, 30_000);"), []);
  assert.deepEqual(reasons("a.test.mjs", "await new Promise((resolve) => setTimeout(resolve, 0));"), []);
  assert.deepEqual(reasons("a.test.mjs", "assert.equal(response.status, 200);"), []);
});

test("accepts a duration whose kind is declared on its line, the line above, or the file", () => {
  assert.deepEqual(reasons("a_tests.rs", "sleep(tick); // timing: poll"), []);
  assert.deepEqual(reasons("a_tests.rs", "// timing: expiry — awaited behind a gate\nrun(Duration::from_millis(100));"), []);
  assert.deepEqual(reasons("a_tests.rs", "// timing-file: mocked — paused clock\n\nsleep(HOUR);"), []);
  assert.deepEqual(reasons("a_tests.rs", "// timing: soon\nsleep(tick);"), ["sleeps"]);
  assert.deepEqual(reasons("a_tests.rs", "// timing: poll\n\nsleep(tick);"), ["sleeps"]);
});
