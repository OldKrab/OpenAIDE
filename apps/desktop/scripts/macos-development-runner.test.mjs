import assert from "node:assert/strict";
import path from "node:path";
import test from "node:test";

import {
  applicationArguments,
  cargoBuildArguments,
  developmentBundleExecutable,
} from "./macos-development-runner.mjs";

test("the macOS runner builds the command Tauri would otherwise run", () => {
  const argumentsFromTauri = ["run", "--no-default-features", "--color", "always", "--", "--example"];
  assert.deepEqual(
    cargoBuildArguments(argumentsFromTauri),
    ["build", "--no-default-features", "--color", "always"],
  );
  assert.deepEqual(applicationArguments(argumentsFromTauri), ["--example"]);
});

test("the macOS runner launches the executable inside the development app bundle", () => {
  assert.equal(
    developmentBundleExecutable("/work/OpenAIDE/apps/desktop/src-tauri", {}),
    path.join(
      "/work/OpenAIDE/apps/desktop/src-tauri",
      "target/debug/bundle/macos/OpenAIDE Development.app/Contents/MacOS/openaide-desktop",
    ),
  );
});
