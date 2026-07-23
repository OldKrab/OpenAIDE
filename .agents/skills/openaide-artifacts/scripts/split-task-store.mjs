import fs from "node:fs";
import path from "node:path";

const JOURNAL_MAGIC = Buffer.from("OAIDETJ\0");
const JOURNAL_HEADER_BYTES = JOURNAL_MAGIC.length + 2;
const MAX_FRAME_BYTES = 256 * 1024 * 1024;

/** Identifies the durable metadata envelope used by task-store-v1. */
export function splitMetadata(taskFile) {
  if (!taskFile || typeof taskFile !== "object" || !taskFile.task || typeof taskFile.chatSnapshot !== "string") {
    return undefined;
  }
  return taskFile;
}

/**
 * Materializes one split Chat without mutating recoverable journal tails.
 * The App Server owns repair; this diagnostic helper must remain read-only.
 */
export function readSplitProjectionMaybe(taskDir, metadata) {
  if (metadata.schemaVersion !== 1) return { error: `${path.join(taskDir, "task.json")}: unsupported Task metadata version` };
  if (typeof metadata.chatJournal !== "string") {
    return { error: `${path.join(taskDir, "task.json")}: missing Chat journal generation` };
  }
  const snapshotFile = path.join(taskDir, metadata.chatSnapshot);
  const snapshot = readJsonMaybe(snapshotFile);
  if (snapshot.error) return snapshot;
  if (!snapshot.value) return { error: `${snapshotFile}: missing Chat snapshot` };
  if (snapshot.value.schemaVersion !== 1) return { error: `${snapshotFile}: unsupported Chat snapshot version` };
  const projection = {
    task: metadata.task,
    messages: structuredClone(snapshot.value.messages ?? []),
    messageMeta: structuredClone(snapshot.value.messageMeta),
    artifactHeads: structuredClone(snapshot.value.artifactHeads ?? {}),
    journal: { frameCount: 0, incompleteTailBytes: 0 },
  };
  const journalFile = path.join(taskDir, metadata.chatJournal);
  if (!fs.existsSync(journalFile)) {
    if (metadata.chatSequence > 0) return { error: `${journalFile}: committed Chat journal is missing` };
    return { value: projection };
  }
  try {
    const journal = readJournalFrames(journalFile);
    if (journal.frames.length < metadata.chatSequence) {
      return { error: `${journalFile}: Chat journal is behind committed Task metadata` };
    }
    for (const frame of journal.frames) applyChatOperations(projection, frame.operations ?? []);
    projection.journal = {
      frameCount: journal.frames.length,
      incompleteTailBytes: journal.incompleteTailBytes,
    };
    return { value: projection };
  } catch (error) {
    return { error: `${journalFile}: ${error.message}` };
  }
}

function readJournalFrames(file) {
  const bytes = fs.readFileSync(file);
  if (bytes.length < JOURNAL_HEADER_BYTES || !bytes.subarray(0, JOURNAL_MAGIC.length).equals(JOURNAL_MAGIC)) {
    throw new Error("invalid journal header");
  }
  if (bytes.readUInt16LE(JOURNAL_MAGIC.length) !== 1) throw new Error("unsupported journal version");
  const frames = [];
  let offset = JOURNAL_HEADER_BYTES;
  while (offset < bytes.length) {
    const frameStart = offset;
    if (bytes.length - offset < 8) return { frames, incompleteTailBytes: bytes.length - frameStart };
    const payloadLength = Number(bytes.readBigUInt64LE(offset));
    offset += 8;
    if (!Number.isSafeInteger(payloadLength) || payloadLength < 0 || payloadLength > MAX_FRAME_BYTES) {
      throw new Error("invalid journal frame length");
    }
    if (offset + payloadLength + 4 > bytes.length) {
      return { frames, incompleteTailBytes: bytes.length - frameStart };
    }
    const payload = bytes.subarray(offset, offset + payloadLength);
    offset += payloadLength;
    const expectedChecksum = bytes.readUInt32LE(offset);
    offset += 4;
    if (crc32(payload) !== expectedChecksum) throw new Error(`journal frame checksum mismatch at sequence ${frames.length + 1}`);
    const frame = JSON.parse(payload.toString("utf8"));
    const expectedSequence = frames.length + 1;
    if (frame.format_version !== 1) throw new Error(`unsupported journal frame version ${frame.format_version}`);
    if (frame.sequence !== expectedSequence) {
      throw new Error(`journal sequence gap: expected ${expectedSequence}, found ${frame.sequence}`);
    }
    frames.push(frame);
  }
  return { frames, incompleteTailBytes: 0 };
}

function applyChatOperations(projection, operations) {
  let historyChanged = false;
  let messageMetaReplaced = false;
  for (const operation of operations) {
    if (operation.operation === "replace_projection") {
      projection.messages = structuredClone(operation.projection.messages ?? []);
      projection.messageMeta = structuredClone(operation.projection.message_meta);
      projection.artifactHeads = structuredClone(operation.projection.artifact_heads ?? {});
    } else if (operation.operation === "append_text") {
      appendText(projection.messages, operation.identity, operation.text);
      projection.messageMeta.local_history_updated_at = operation.local_history_updated_at;
      historyChanged = true;
    } else if (operation.operation === "append_message") {
      projection.messages.push(structuredClone(operation.message));
      historyChanged = true;
    } else if (operation.operation === "upsert_message") {
      const index = projection.messages.findIndex((stored) => stored.chat?.identity === operation.message.chat?.identity);
      if (index >= 0) projection.messages[index] = structuredClone(operation.message);
      else projection.messages.push(structuredClone(operation.message));
      historyChanged = true;
    } else if (operation.operation === "replace_messages") {
      projection.messages = structuredClone(operation.messages ?? []);
      projection.messageMeta = structuredClone(operation.message_meta);
      messageMetaReplaced = true;
    } else if (operation.operation === "replace_message_meta") {
      projection.messageMeta = structuredClone(operation.message_meta);
      messageMetaReplaced = true;
    } else if (operation.operation === "commit_artifact") {
      projection.artifactHeads[operation.artifact_id] = operation.artifact_sequence;
    }
  }
  if (historyChanged && !messageMetaReplaced) projection.messageMeta.version += 1;
}

function appendText(messages, identity, text) {
  const stored = messages.find((candidate) => candidate.chat?.identity === identity);
  const parts = stored?.chat?.message?.parts;
  const last = Array.isArray(parts) ? parts.at(-1) : undefined;
  if (!last || last.kind !== "text") throw new Error(`append_text target is missing: ${identity}`);
  last.text += text;
}

function readJsonMaybe(file) {
  if (!fs.existsSync(file)) return { value: undefined };
  try {
    return { value: JSON.parse(fs.readFileSync(file, "utf8")) };
  } catch (error) {
    return { error: `${file}: ${error.message}` };
  }
}

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
    }
  }
  return (~crc) >>> 0;
}
