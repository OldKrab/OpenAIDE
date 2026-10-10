import {
  DEVICES_APPROVE_JOIN_REQUEST,
  DEVICES_CANCEL_INVITE,
  DEVICES_CREATE_INVITE,
  DEVICES_PREVIEW_JOIN_REQUEST,
  DEVICES_REMOVE,
  type BackendConnection,
  type DevicesCreateInviteResult,
  type DevicesPreviewJoinRequestResult,
} from "@openaide/app-server-client";

type RemoteDeviceConnection = Pick<BackendConnection, "request">;

/**
 * Pairing and removal of Remote Devices. App Server owns the trusted list, so these only send
 * intent; the resulting list arrives through the Devices subscription.
 */
export type RemoteDeviceIntents = {
  /** Replaces any Pairing Code this App Server was showing with a new single-use one. */
  createInvite(): Promise<DevicesCreateInviteResult>;
  cancelInvite(): Promise<void>;
  /** Decodes a device's join code without trusting it, for the confirmation step. */
  previewJoinRequest(code: string): Promise<DevicesPreviewJoinRequestResult>;
  approveJoinRequest(code: string): Promise<void>;
  remove(deviceId: string): Promise<void>;
};

export function createRemoteDeviceIntents(connection?: RemoteDeviceConnection): RemoteDeviceIntents {
  const required = () => {
    if (!connection) throw new Error("Devices require the App Server.");
    return connection;
  };
  return {
    createInvite: async () => required().request(DEVICES_CREATE_INVITE, {}),
    cancelInvite: async () => {
      await required().request(DEVICES_CANCEL_INVITE, {});
    },
    previewJoinRequest: async (code) => required().request(DEVICES_PREVIEW_JOIN_REQUEST, { code }),
    approveJoinRequest: async (code) => {
      await required().request(DEVICES_APPROVE_JOIN_REQUEST, { code });
    },
    remove: async (deviceId) => {
      await required().request(DEVICES_REMOVE, { deviceId });
    },
  };
}
