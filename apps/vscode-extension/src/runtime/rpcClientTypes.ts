export type PendingRequest = {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timeout: NodeJS.Timeout;
};

export type RuntimeNotification = {
  method: string;
  params?: unknown;
};

export type RuntimeHostRequestHandler = (params: unknown) => Promise<unknown> | unknown;
