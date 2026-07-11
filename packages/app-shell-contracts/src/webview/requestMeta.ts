export type WebviewSnapshotMeta = {
  snapshot_request_id?: number;
  snapshot_intent?: "open" | "refresh";
};

export type WebviewOptionsRequestMeta = {
  options_request_key?: string;
};

export type WebviewSessionListRequestMeta = {
  session_list_request_id?: number;
  session_list_request_key?: string;
  append?: boolean;
};

