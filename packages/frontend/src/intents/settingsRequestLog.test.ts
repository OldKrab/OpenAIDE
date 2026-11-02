import { AppServerProtocolError, SETTINGS_UPDATE_RUNTIME } from "@openaide/app-server-client";
import { afterEach, expect, it, vi } from "vitest";
import { withSettingsRequestLog } from "./settingsRequestLog";

afterEach(() => vi.restoreAllMocks());

const params = { developer: { acpTrace: { enabled: true } } };

it("logs the start and end of a Settings request without its content", async () => {
  const info = vi.spyOn(console, "info").mockImplementation(() => undefined);
  const result = { secret: "private-result" };
  const request = vi.fn().mockResolvedValue(result);

  await expect(withSettingsRequestLog({ request }).request(SETTINGS_UPDATE_RUNTIME, params)).resolves.toBe(result);

  expect(request).toHaveBeenCalledWith(SETTINGS_UPDATE_RUNTIME, params);
  const [started, completed] = info.mock.calls.map(([line]) => String(line));
  expect(started).toMatch(new RegExp(`^settings_request_started operation=${SETTINGS_UPDATE_RUNTIME} operation_id=\\S+ attempt=1$`));
  expect(completed).toMatch(new RegExp(`^settings_request_completed operation=${SETTINGS_UPDATE_RUNTIME} operation_id=\\S+ attempt=1 outcome=success duration_ms=\\d+$`));
  expect(started.split(" ")[2]).toBe(completed.split(" ")[2]);
  expect(`${started}${completed}`).not.toContain("private-result");
});

it("classifies a failed Settings request and leaves its message out of the log", async () => {
  vi.spyOn(console, "info").mockImplementation(() => undefined);
  const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
  const refused = new AppServerProtocolError({ error: { code: "invalidRequest", message: "private detail" } } as never);

  await expect(withSettingsRequestLog({ request: vi.fn().mockRejectedValue(refused) }).request(SETTINGS_UPDATE_RUNTIME, params)).rejects.toBe(refused);
  await expect(withSettingsRequestLog({ request: vi.fn().mockRejectedValue(new Error("private detail")) }).request(SETTINGS_UPDATE_RUNTIME, params)).rejects.toThrow();

  const lines = warn.mock.calls.map(([line]) => String(line));
  expect(lines[0]).toMatch(/outcome=failure duration_ms=\d+ error_kind=invalidRequest$/);
  expect(lines[1]).toMatch(/outcome=failure duration_ms=\d+ error_kind=transport$/);
  expect(lines.join("")).not.toContain("private detail");
});
