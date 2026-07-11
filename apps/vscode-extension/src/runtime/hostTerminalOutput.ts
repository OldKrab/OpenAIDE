import type { StringDecoder } from "node:string_decoder";
import type { TerminalRecord } from "./hostTerminalTypes";

export function appendOutput(record: TerminalRecord, decoder: StringDecoder, chunk: Buffer) {
  record.output += decoder.write(chunk);
  truncateOutput(record);
}

export function appendDecoderRemainder(record: TerminalRecord) {
  const stdoutTail = record.stdoutDecoder.end();
  const stderrTail = record.stderrDecoder.end();
  record.output += stdoutTail;
  record.output += stderrTail;
  truncateOutput(record);
}

function truncateOutput(record: TerminalRecord) {
  if (record.outputByteLimit === 0) {
    record.output = "";
    record.truncated = true;
    return;
  }

  const bytes = Buffer.from(record.output, "utf8");
  if (bytes.byteLength <= record.outputByteLimit) return;

  let start = bytes.byteLength - record.outputByteLimit;
  while (start < bytes.byteLength && (bytes[start] & 0b1100_0000) === 0b1000_0000) {
    start += 1;
  }
  record.output = bytes.subarray(start).toString("utf8");
  record.truncated = true;
}
