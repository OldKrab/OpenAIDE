import {
  CLIENT_REPLACEMENT_IMPACT,
  DEVICES_CANCEL_INVITE,
  DEVICES_CREATE_INVITE,
  DEVICES_RELAY_ADD_PERSON,
  DEVICES_RELAY_PEOPLE,
  DEVICES_RELAY_REVOKE_PERSON,
  DEVICES_REMOVE,
  DEVICES_SET_RELAY,
  type BackendConnection,
  type ClientReplacementImpactResult,
  type DevicesCreateInviteResult,
  type DevicesRelayAddPersonResult,
  type DevicesSetRelayResult,
  type RelayPerson,
} from "@openaide/app-server-client";

type RemoteDeviceConnection = Pick<BackendConnection, "request">;

/**
 * Pairing and removal of Remote Devices, and the Own Relay they connect through (ADR-0085).
 * App Server owns the trusted list and the relay, so these only send intent; the resulting
 * list and relay arrive through the Devices subscription. The relay's people are not in that
 * snapshot, so they are read on request.
 */
export type RemoteDeviceIntents = {
  /** Replaces any Pairing Code this App Server was showing with a new single-use one. */
  createInvite(): Promise<DevicesCreateInviteResult>;
  cancelInvite(): Promise<void>;
  remove(deviceId: string): Promise<void>;
  /** What replacing this App Server would interrupt, other than this client. */
  replacementImpact(): Promise<ClientReplacementImpactResult>;
  /**
   * Uses the Own Relay a relay key names, or the free public relays when `relayKey` is absent.
   * Any change removes every paired device. The key is a secret: callers never log it.
   */
  setRelay(relayKey: string | undefined): Promise<DevicesSetRelayResult>;
  /** The people the Own Relay admits; only an App Server whose relay key can manage may ask. */
  relayPeople(): Promise<RelayPerson[]>;
  /** Makes a relay key for one more person. The result's key is shown once and never logged. */
  relayAddPerson(name: string, canManage: boolean): Promise<DevicesRelayAddPersonResult>;
  relayRevokePerson(name: string): Promise<void>;
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
    remove: async (deviceId) => {
      await required().request(DEVICES_REMOVE, { deviceId });
    },
    replacementImpact: async () => required().request(CLIENT_REPLACEMENT_IMPACT, {}),
    // An absent relay means the free public relays.
    setRelay: async (relayKey) => required().request(DEVICES_SET_RELAY, relayKey === undefined ? {} : { relay: relayKey }),
    relayPeople: async () => (await required().request(DEVICES_RELAY_PEOPLE, {})).people,
    relayAddPerson: async (name, canManage) => required().request(DEVICES_RELAY_ADD_PERSON, { name, canManage }),
    relayRevokePerson: async (name) => {
      await required().request(DEVICES_RELAY_REVOKE_PERSON, { name });
    },
  };
}
