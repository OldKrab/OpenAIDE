import assert from "node:assert/strict";
import { chmodSync, cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn, spawnSync } from "node:child_process";
import test from "node:test";

test("refresh reuses the owned listener when its wrapper pid is dead", async (t) => {
  const fixtureRoot = mkdtempSync(join(tmpdir(), "openaide-local-web-refresh-"));
  const stateRoot = join(fixtureRoot, "state");
  const runtimeRoot = join(fixtureRoot, "runtime");
  const staticRoot = join(fixtureRoot, "static");
  const npmLog = join(fixtureRoot, "npm.log");
  const pidFile = join(fixtureRoot, "dead-wrapper.pid");
  const fakeBin = join(fixtureRoot, "bin");
  const fakeNpm = join(fakeBin, "npm");
  mkdirSync(fakeBin);
  writeFileSync(fakeNpm, `#!/usr/bin/env bash\nprintf '%s\\n' "$*" >> "$OPENAIDE_TEST_NPM_LOG"\n`);
  chmodSync(fakeNpm, 0o755);
  writeFileSync(pidFile, "99999999\n");

  const listener = spawn(process.execPath, ["-e", `
    const net = require("node:net");
    const server = net.createServer((socket) => socket.end());
    server.listen(0, "127.0.0.1", () => console.log(server.address().port));
    process.on("SIGTERM", () => server.close(() => process.exit(0)));
  `], {
    env: {
      ...process.env,
      OPENAIDE_WEB_STATE_ROOT: stateRoot,
      OPENAIDE_WEB_RUNTIME_ROOT: runtimeRoot,
      OPENAIDE_WEB_STATIC_ROOT: staticRoot,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(() => {
    listener.kill("SIGTERM");
    rmSync(fixtureRoot, { recursive: true, force: true });
  });
  const port = await firstOutputLine(listener);

  const result = spawnSync("bash", ["deploy/local-web.sh", "refresh"], {
    cwd: new URL("..", import.meta.url),
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${fakeBin}:${process.env.PATH}`,
      OPENAIDE_TEST_NPM_LOG: npmLog,
      OPENAIDE_WEB_ROLE: "",
      OPENAIDE_WEB_HOST: "127.0.0.1",
      OPENAIDE_WEB_PORT: port,
      OPENAIDE_WEB_VITE_PORT: port,
      OPENAIDE_WEB_ALLOWED_HOSTS: "localhost,127.0.0.1",
      OPENAIDE_WEB_STATE_ROOT: stateRoot,
      OPENAIDE_WEB_RUNTIME_ROOT: runtimeRoot,
      OPENAIDE_WEB_STATIC_ROOT: staticRoot,
      OPENAIDE_WEB_PID_FILE: pidFile,
      OPENAIDE_WEB_LOG_FILE: join(fixtureRoot, "web.log"),
      OPENAIDE_WEB_SKIP_BUILD: "1",
      OPENAIDE_WEB_DAEMON: "background",
    },
  });

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /OpenAIDE local web refreshed/);
  assert.doesNotMatch(result.stdout, /not running; starting it/);
  assert.doesNotMatch(readFileSync(npmLog, "utf8"), /run web:dev/);
  assert.equal(listener.exitCode, null);
});

function firstOutputLine(child) {
  return new Promise((resolve, reject) => {
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
      const newline = stdout.indexOf("\n");
      if (newline !== -1) resolve(stdout.slice(0, newline).trim());
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.once("exit", (code) => reject(new Error(`listener exited with ${code}: ${stderr}`)));
    child.once("error", reject);
  });
}

test("local web preview env overrides deploy/local-web.env", () => {
  const result = spawnSync("bash", ["deploy/local-web.sh", "status"], {
    cwd: new URL("..", import.meta.url),
    encoding: "utf8",
    env: {
      ...process.env,
      OPENAIDE_WEB_PORT: "5599",
      OPENAIDE_WEB_VITE_PORT: "5598",
      OPENAIDE_WEB_STATIC_ROOT: "",
      OPENAIDE_WEB_PID_FILE: "/tmp/openaide-local-web-test-5599.pid",
    },
  });

  assert.equal(result.status, 0, result.stderr);
  assert.doesNotMatch(result.stdout, /:5474\b/);
  assert.match(result.stdout, /\.openaide-web-dev\/static-5599/);
});

test("local web role loads its role-specific env file", (t) => {
  const fixtureRoot = mkdtempSync(join(tmpdir(), "openaide-local-web-role-status-"));
  const fakeBin = join(fixtureRoot, "bin");
  const fakeSystemctl = join(fakeBin, "systemctl");
  mkdirSync(fakeBin);
  writeFileSync(fakeSystemctl, `#!/usr/bin/env bash
if [[ "$*" == *"--property MainPID --value"* ]]; then printf '4242\\n'; fi
exit 0
`);
  chmodSync(fakeSystemctl, 0o755);
  t.after(() => rmSync(fixtureRoot, { recursive: true, force: true }));

  const result = spawnSync("bash", ["deploy/local-web.sh", "status"], {
    cwd: new URL("..", import.meta.url),
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${fakeBin}:${process.env.PATH}`,
      OPENAIDE_WEB_ROLE: "target",
    },
  });

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /:5574\b/);
  assert.match(result.stdout, /\.openaide-web-target\/static/);
});

test("local web role loads an ignored role-local env after tracked role defaults", (t) => {
  const fixtureRoot = mkdtempSync(join(tmpdir(), "openaide-local-web-role-local-"));
  const fixtureDeploy = join(fixtureRoot, "deploy");
  cpSync(new URL(".", import.meta.url), fixtureDeploy, { recursive: true });
  t.after(() => rmSync(fixtureRoot, { recursive: true, force: true }));

  writeFileSync(join(fixtureDeploy, "local-web.fixture.env"), "OPENAIDE_WEB_PORT=5580\n");
  writeFileSync(join(fixtureDeploy, "local-web.fixture.local.env"), "OPENAIDE_WEB_PORT=5591\n");

  const result = spawnSync("bash", [join(fixtureDeploy, "local-web.sh"), "status"], {
    cwd: fixtureRoot,
    encoding: "utf8",
    env: {
      ...process.env,
      OPENAIDE_WEB_ROLE: "fixture",
      OPENAIDE_WEB_DAEMON: "background",
    },
  });

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /:5591\b/);
  assert.doesNotMatch(result.stdout, /:5580\b/);
});

test("target role uses a durable isolated systemd service", () => {
  const targetEnv = readFileSync(new URL("./local-web.target.env", import.meta.url), "utf8");

  assert.match(targetEnv, /^OPENAIDE_WEB_DAEMON=systemd$/m);
  assert.match(targetEnv, /^OPENAIDE_WEB_SYSTEMD_UNIT=openaide-web-target-5574$/m);
  assert.match(targetEnv, /^OPENAIDE_WEB_PROTOTYPE_PORT=5572$/m);
  assert.doesNotMatch(targetEnv, /OPENAIDE_WEB_PROTOTYPE_ROOT/);
});

test("driver role uses a durable isolated systemd service", () => {
  const driverEnv = readFileSync(new URL("./local-web.driver.env", import.meta.url), "utf8");

  assert.match(driverEnv, /^OPENAIDE_WEB_ALLOWED_HOSTS=localhost,127\.0\.0\.1$/m);
  assert.match(driverEnv, /^OPENAIDE_WEB_PORT=5474$/m);
  assert.match(driverEnv, /^OPENAIDE_WEB_VITE_PORT=5473$/m);
  assert.doesNotMatch(driverEnv, /OPENAIDE_WEB_PROTOTYPE_PORT/);
  assert.match(driverEnv, /^OPENAIDE_WEB_STATE_ROOT=.*\/\.openaide-web-dev\/state$/m);
  assert.match(driverEnv, /^OPENAIDE_WEB_RUNTIME_ROOT=.*\/\.openaide-web-dev\/runtime$/m);
  assert.match(driverEnv, /^OPENAIDE_WEB_STATIC_ROOT=.*\/\.openaide-web-dev\/static-5474$/m);
  assert.match(driverEnv, /^OPENAIDE_WEB_PID_FILE=\/tmp\/openaide-web-driver-5474\.pid$/m);
  assert.match(driverEnv, /^OPENAIDE_WEB_LOG_FILE=\/tmp\/openaide-web-driver-5474\.log$/m);
  assert.match(driverEnv, /^OPENAIDE_WEB_DAEMON=systemd$/m);
  assert.match(driverEnv, /^OPENAIDE_WEB_SYSTEMD_UNIT=openaide-web-driver-5474$/m);
});

test("systemd restart delegates outside the service that is restarting itself", () => {
  const fixtureRoot = mkdtempSync(join(tmpdir(), "openaide-local-web-self-restart-"));
  const fakeBin = join(fixtureRoot, "bin");
  const staticRoot = join(fixtureRoot, "static");
  const systemctlLog = join(fixtureRoot, "systemctl.log");
  const systemdRunLog = join(fixtureRoot, "systemd-run.log");
  const cgroupFile = join(fixtureRoot, "cgroup");
  mkdirSync(fakeBin);
  mkdirSync(staticRoot);
  writeFileSync(join(staticRoot, "index.html"), "ready\n");
  writeFileSync(cgroupFile, "0::/user.slice/user-1000.slice/app.slice/openaide-web-test.service\n");
  writeFileSync(join(fakeBin, "systemctl"), `#!/usr/bin/env bash
printf '%s\n' "$*" >> "$OPENAIDE_TEST_SYSTEMCTL_LOG"
exit 0
`);
  writeFileSync(join(fakeBin, "systemd-run"), `#!/usr/bin/env bash
printf '%s\n' "$*" >> "$OPENAIDE_TEST_SYSTEMD_RUN_LOG"
exit 0
`);
  chmodSync(join(fakeBin, "systemctl"), 0o755);
  chmodSync(join(fakeBin, "systemd-run"), 0o755);

  const result = spawnSync("bash", ["deploy/local-web.sh", "restart"], {
    cwd: new URL("..", import.meta.url),
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${fakeBin}:${process.env.PATH}`,
      OPENAIDE_TEST_SYSTEMCTL_LOG: systemctlLog,
      OPENAIDE_TEST_SYSTEMD_RUN_LOG: systemdRunLog,
      OPENAIDE_WEB_ROLE: "",
      OPENAIDE_WEB_ALLOWED_HOSTS: "localhost,127.0.0.1",
      OPENAIDE_WEB_CURRENT_CGROUP_FILE: cgroupFile,
      OPENAIDE_WEB_DAEMON: "systemd",
      OPENAIDE_WEB_SYSTEMD_UNIT: "openaide-web-test",
      OPENAIDE_WEB_STATIC_ROOT: staticRoot,
      OPENAIDE_WEB_BUILD: "0",
      OPENAIDE_WEB_SKIP_BUILD: "1",
    },
  });
  const systemctlCalls = readFileSync(systemctlLog, "utf8");
  const systemdRunCalls = readFileSync(systemdRunLog, "utf8");
  rmSync(fixtureRoot, { recursive: true, force: true });

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Restart delegated outside openaide-web-test\.service/);
  assert.doesNotMatch(systemctlCalls, /stop openaide-web-test\.service/);
  assert.match(systemdRunCalls, /--unit openaide-web-test-restart-/);
  assert.match(systemdRunCalls, /OPENAIDE_WEB_RESTART_HELPER=1/);
});

test("systemd status reports the current service pid instead of a stale wrapper pid", () => {
  const fixtureRoot = mkdtempSync(join(tmpdir(), "openaide-local-web-status-"));
  const fakeBin = join(fixtureRoot, "bin");
  const fakeSystemctl = join(fakeBin, "systemctl");
  mkdirSync(fakeBin);
  writeFileSync(fakeSystemctl, `#!/usr/bin/env bash
if [[ "$*" == *"is-active"* ]]; then exit 0; fi
if [[ "$*" == *"--property MainPID --value"* ]]; then printf '4242\\n'; exit 0; fi
exit 0
`);
  chmodSync(fakeSystemctl, 0o755);

  const result = spawnSync("bash", ["deploy/local-web.sh", "status"], {
    cwd: new URL("..", import.meta.url),
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${fakeBin}:${process.env.PATH}`,
      OPENAIDE_WEB_ROLE: "",
      OPENAIDE_WEB_DAEMON: "systemd",
      OPENAIDE_WEB_SYSTEMD_UNIT: "openaide-web-test",
      OPENAIDE_WEB_PORT: "5599",
      OPENAIDE_WEB_VITE_PORT: "5598",
      OPENAIDE_WEB_PID_FILE: join(fixtureRoot, "stale.pid"),
    },
  });
  rmSync(fixtureRoot, { recursive: true, force: true });

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /service: running pid 4242/);
  assert.doesNotMatch(result.stdout, /wrapper: not running/);
});

test("systemd status fails when the managed service is not running", () => {
  const fixtureRoot = mkdtempSync(join(tmpdir(), "openaide-local-web-unhealthy-status-"));
  const fakeBin = join(fixtureRoot, "bin");
  const fakeSystemctl = join(fakeBin, "systemctl");
  mkdirSync(fakeBin);
  writeFileSync(fakeSystemctl, `#!/usr/bin/env bash
if [[ "$*" == *"show-environment"* ]]; then exit 0; fi
if [[ "$*" == *"is-active"* ]]; then exit 3; fi
exit 0
`);
  chmodSync(fakeSystemctl, 0o755);

  const result = spawnSync("bash", ["deploy/local-web.sh", "status"], {
    cwd: new URL("..", import.meta.url),
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${fakeBin}:${process.env.PATH}`,
      OPENAIDE_WEB_ROLE: "",
      OPENAIDE_WEB_DAEMON: "systemd",
      OPENAIDE_WEB_SYSTEMD_UNIT: "openaide-web-test",
      OPENAIDE_WEB_PORT: "5599",
      OPENAIDE_WEB_VITE_PORT: "5598",
      OPENAIDE_WEB_PID_FILE: join(fixtureRoot, "stale.pid"),
    },
  });
  rmSync(fixtureRoot, { recursive: true, force: true });

  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /service: not running/);
});

test("default local web script refreshes without restarting the active App Server", () => {
  const pkg = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));

  assert.equal(pkg.scripts["web:local"], "bash deploy/local-web.sh refresh");
  assert.match(pkg.scripts["web:local:restart"], /deploy\/local-web\.sh restart/);
  assert.equal(pkg.scripts["web:target:restart"], "OPENAIDE_WEB_ROLE=target bash deploy/local-web.sh restart");
  assert.equal(pkg.scripts["web:driver:restart"], "OPENAIDE_WEB_ROLE=driver bash deploy/local-web.sh restart");
  assert.match(pkg.scripts["web:preview"], /OPENAIDE_WEB_PORT=5574/);
  assert.match(pkg.scripts["web:preview"], /OPENAIDE_WEB_STATIC_ROOT=\.openaide-web-preview\/static/);
});

test("local web builds frontend assets into the configured static root", () => {
  const source = readFileSync(new URL("./local-web.sh", import.meta.url), "utf8");
  const staticPolicy = readFileSync(new URL("./local-web-static.sh", import.meta.url), "utf8");

  assert.match(source, /--outDir "\$static_root"/);
  assert.match(source, /--emptyOutDir/);
  assert.match(staticPolicy, /Refusing to use source frontend dist as a local web static root/);
});

test("local web can run an opt-in send-path smoke check after start", () => {
  const script = readFileSync(new URL("./local-web.sh", import.meta.url), "utf8");
  const smoke = readFileSync(new URL("./local-web-smoke.mjs", import.meta.url), "utf8");

  assert.match(script, /OPENAIDE_WEB_SMOKE/);
  assert.match(script, /smoke_if_requested/);
  assert.match(script, /local-web-smoke\.mjs" "http:\/\/\$host:\$port"/);
  assert.match(smoke, /\/__openaide-app-server\/probe/);
  assert.match(smoke, /"task\/create"/);
  assert.match(smoke, /"task\/send"/);
  assert.match(smoke, /"task\/cancel"/);
  assert.match(smoke, /"task\/setArchived"/);
});

test("local web supports durable user systemd daemon mode", () => {
  const script = readFileSync(new URL("./local-web.sh", import.meta.url), "utf8");

  assert.match(script, /OPENAIDE_WEB_DAEMON/);
  assert.match(script, /OPENAIDE_WEB_SYSTEMD_UNIT/);
  assert.match(script, /background\|systemd/);
  assert.match(script, /systemd-run --user/);
  assert.match(script, /Restart=always/);
  assert.match(script, /systemctl --user status "\$systemd_unit\.service"/);
});

test("local web enables JavaScript source maps for background and systemd servers", () => {
  const script = readFileSync(new URL("./local-web.sh", import.meta.url), "utf8");

  assert.match(script, /node_options=.*NODE_OPTIONS/);
  assert.match(script, /node_options=.*--enable-source-maps/);
  assert.match(script, /setsid env[\s\S]*NODE_OPTIONS="\$node_options"[\s\S]*npm run web:dev/);
  assert.match(
    script,
    /systemd-run --user[\s\S]*--setenv "NODE_OPTIONS=\$node_options"[\s\S]*"\$npm_bin" run web:dev/,
  );
});

test("local web forwards ACP trace settings into background and systemd app servers", () => {
  const script = readFileSync(new URL("./local-web.sh", import.meta.url), "utf8");

  assert.match(script, /OPENAIDE_ACP_TRACE/);
  assert.match(script, /OPENAIDE_ACP_TRACE_DIR/);
  assert.match(script, /trace_env_args=\(\)/);
  assert.match(script, /systemd_trace_env_args=\(\)/);
  assert.match(script, /\[\[ -n "\$\{OPENAIDE_ACP_TRACE_DIR:-\}" \]\]/);
  assert.doesNotMatch(script, /OPENAIDE_ACP_TRACE_DIR="\$\{OPENAIDE_ACP_TRACE_DIR:-\}"/);
  assert.doesNotMatch(script, /--setenv "OPENAIDE_ACP_TRACE_DIR=\$\{OPENAIDE_ACP_TRACE_DIR:-\}"/);
});

test("local web serializes mutating lifecycle commands per port", () => {
  const script = readFileSync(new URL("./local-web.sh", import.meta.url), "utf8");

  assert.match(script, /lock_file=.*openaide-local-web-\$port\.lock/);
  assert.match(script, /flock 9/);
  assert.match(script, /npm run web:dev 9>&-/);
  assert.match(script, /with_lifecycle_lock refresh_server/);
  assert.match(script, /with_lifecycle_lock restart_server/);
});
