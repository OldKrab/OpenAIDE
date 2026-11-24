//! Methods that pair and remove Remote Devices.

use super::{ProtocolMethod, DEVICES_CANCEL_INVITE, DEVICES_CREATE_INVITE, DEVICES_REMOVE};
use crate::devices::{
    DevicesCancelInviteParams, DevicesCancelInviteResult, DevicesCreateInviteParams,
    DevicesCreateInviteResult, DevicesRemoveParams, DevicesRemoveResult,
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
