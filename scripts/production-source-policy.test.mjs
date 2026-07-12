import assert from "node:assert/strict";
import test from "node:test";

import { isProductionSource, logicalLineCount } from "./production-source-policy.mjs";

test("classifies flat Rust unit-test files as tests", () => {
  assert.equal(isProductionSource("openaide-rs/app-server/src/tasks/product_api_tests.rs"), false);
  assert.equal(isProductionSource("openaide-rs/app-server/src/tasks/product_api.rs"), true);
});

test("retains existing test file and directory exclusions", () => {
  assert.equal(isProductionSource("openaide-rs/app-server/src/tasks/product_api/tests.rs"), false);
  assert.equal(isProductionSource("openaide-rs/app-server/src/tasks/tests/scenario.rs"), false);
});

test("counts nonblank logical lines", () => {
  assert.equal(logicalLineCount("one\n\n  \ntwo\n"), 2);
});
