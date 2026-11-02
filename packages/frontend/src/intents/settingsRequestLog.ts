import { AppServerProtocolError, type BackendConnection } from "@openaide/app-server-client";

type SettingsConnection = Pick<BackendConnection, "request">;

/**
 * Logs each Settings request's start and end, so a slow or failed action can be
 * told apart from one that never left the Frontend. Every Settings intent goes
 * through this one seam. Only the method name and outcome are logged: parameters
 * and messages can hold secrets, commands, and paths.
 */
export function withSettingsRequestLog({ request }: SettingsConnection): SettingsConnection {
  const logged = async (...args: Parameters<SettingsConnection["request"]>) => {
    const line = `operation=${args[0]} operation_id=${crypto.randomUUID()} attempt=1`;
    const start = performance.now();
    console.info(`settings_request_started ${line}`);
    try {
      const result = await request(...args);
      console.info(`settings_request_completed ${line} outcome=success duration_ms=${elapsed(start)}`);
      return result;
    } catch (error) {
      console.warn(`settings_request_completed ${line} outcome=failure duration_ms=${elapsed(start)} error_kind=${errorKind(error)}`);
      throw error;
    }
  };
  return { request: logged as SettingsConnection["request"] };
}

function errorKind(error: unknown) {
  return error instanceof AppServerProtocolError ? error.protocolError.code : "transport";
}

function elapsed(start: number) {
  return Math.round(performance.now() - start);
}
