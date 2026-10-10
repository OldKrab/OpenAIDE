import assert from "node:assert/strict";
import test from "node:test";
import { bindingsHash, declaredVersion, versionProblem } from "./check-app-server-protocol-version.mjs";

const released = { version: "2.1", bindingsSha256: bindingsHash("released bindings") };

test("reads the declared version from the Rust constant", () => {
  const source = "pub const APP_SERVER_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion { major: 3, minor: 12 };";
  assert.deepEqual(declaredVersion(source), { major: 3, minor: 12 });
  assert.throws(() => declaredVersion("pub const OTHER: u8 = 1;"), /was not found/);
});

test("unchanged bindings need no bump", () => {
  assert.equal(versionProblem({ released, current: { major: 2, minor: 1 }, hash: released.bindingsSha256 }), null);
});

test("changed bindings need a higher minor or major", () => {
  const hash = bindingsHash("changed bindings");
  assert.match(versionProblem({ released, current: { major: 2, minor: 1 }, hash }), /still declares 2\.1/);
  assert.match(versionProblem({ released, current: { major: 2, minor: 0 }, hash }), /still declares 2\.0/);
  assert.equal(versionProblem({ released, current: { major: 2, minor: 2 }, hash }), null);
  assert.equal(versionProblem({ released, current: { major: 3, minor: 0 }, hash }), null);
});
