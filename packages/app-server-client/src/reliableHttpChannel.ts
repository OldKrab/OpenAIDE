import { createDiagnosticsLogger } from "./diagnostics.js";
import { createHttpLinkOpener, type HttpTransportLinkOptions } from "./httpTransportLink.js";
import {
  createReliableSessionChannel,
  type ReliableSessionChannel,
} from "./reliableSessionChannel.js";

export {
  reliableHttpErrorDiagnosticFields,
  type ReliableHttpFetch,
} from "./httpTransportLink.js";

export type ReliableHttpMessageChannel = ReliableSessionChannel;

export type ReliableHttpMessageChannelOptions = HttpTransportLinkOptions;

/** Composes the reliable session with its HTTP link as one message channel. */
export function createReliableHttpMessageChannel(
  options: ReliableHttpMessageChannelOptions,
): ReliableHttpMessageChannel {
  const logger = options.logger ?? createDiagnosticsLogger("openaide-reliable-http");
  return createReliableSessionChannel({
    openLink: createHttpLinkOpener({ ...options, logger }),
    connectionId: options.connectionId,
    logger,
    ...(options.retryDelayMs === undefined ? {} : { retryDelayMs: options.retryDelayMs }),
  });
}
