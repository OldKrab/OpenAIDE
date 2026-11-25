//! Methods that pair and remove Remote Devices.

use super::{
    ProtocolMethod, DEVICES_CANCEL_INVITE, DEVICES_CREATE_INVITE, DEVICES_RELAY_ADD_PERSON,
    DEVICES_RELAY_PEOPLE, DEVICES_RELAY_REVOKE_PERSON, DEVICES_REMOVE, DEVICES_SET_RELAY,
};
use crate::devices::{
    DevicesCancelInviteParams, DevicesCancelInviteResult, DevicesCreateInviteParams,
    DevicesCreateInviteResult, DevicesRelayAddPersonParams, DevicesRelayAddPersonResult,
    DevicesRelayPeopleParams, DevicesRelayPeopleResult, DevicesRelayRevokePersonParams,
    DevicesRelayRevokePersonResult, DevicesRemoveParams, DevicesRemoveResult,
    DevicesSetRelayParams, DevicesSetRelayResult,
};

protocol_method!(
    DevicesCreateInvite,
    DEVICES_CREATE_INVITE,
    DevicesCreateInviteParams,
    DevicesCreateInviteResult
);
protocol_method!(
    DevicesCancelInvite,
    DEVICES_CANCEL_INVITE,
    DevicesCancelInviteParams,
    DevicesCancelInviteResult
);
protocol_method!(
    DevicesRemove,
    DEVICES_REMOVE,
    DevicesRemoveParams,
    DevicesRemoveResult
);
protocol_method!(
    DevicesSetRelay,
    DEVICES_SET_RELAY,
    DevicesSetRelayParams,
    DevicesSetRelayResult
);
protocol_method!(
    DevicesRelayPeople,
    DEVICES_RELAY_PEOPLE,
    DevicesRelayPeopleParams,
    DevicesRelayPeopleResult
);
protocol_method!(
    DevicesRelayAddPerson,
    DEVICES_RELAY_ADD_PERSON,
    DevicesRelayAddPersonParams,
    DevicesRelayAddPersonResult
);
protocol_method!(
    DevicesRelayRevokePerson,
    DEVICES_RELAY_REVOKE_PERSON,
    DevicesRelayRevokePersonParams,
    DevicesRelayRevokePersonResult
);
