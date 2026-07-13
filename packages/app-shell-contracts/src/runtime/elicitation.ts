export type ElicitationValue = string | number | boolean | string[];

export type ElicitationOption = {
  value: string;
  label: string;
  description?: string;
};

type ElicitationFieldBase = {
  id: string;
  label: string;
  description?: string;
  required: boolean;
};

export type ElicitationField =
  | (ElicitationFieldBase & {
      kind: "string";
      default_value?: string;
      format?: "text" | "multiline" | "email" | "uri" | "date" | "date-time";
      min_length?: number;
      max_length?: number;
      pattern?: string;
    })
  | (ElicitationFieldBase & {
      kind: "number";
      default_value?: number;
      minimum?: number;
      maximum?: number;
    })
  | (ElicitationFieldBase & {
      kind: "integer";
      default_value?: number;
      minimum?: number;
      maximum?: number;
    })
  | (ElicitationFieldBase & {
      kind: "boolean";
      default_value?: boolean;
    })
  | (ElicitationFieldBase & {
      kind: "singleSelect";
      default_value?: string;
      options: ElicitationOption[];
    })
  | (ElicitationFieldBase & {
      kind: "multiSelect";
      default_value?: string[];
      min_items?: number;
      max_items?: number;
      options: ElicitationOption[];
    });

export type ElicitationAnswer = {
  field_id: string;
  label: string;
  value: ElicitationValue;
};

export type ElicitationMessage = {
  kind: "elicitation";
  id: string;
  request_id: string;
  app_server_request_id?: string;
  prompt: string;
  state: "pending" | "resolved" | "cancelled" | "error";
  created_at: string;
  fields: ElicitationField[];
  answers?: ElicitationAnswer[];
  error?: string;
  resolution_message?: string;
};

export type ElicitationResponse =
  | { action: "submit"; content: Record<string, ElicitationValue> }
  | { action: "cancel" };
