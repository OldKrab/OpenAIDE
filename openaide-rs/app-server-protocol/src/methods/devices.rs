//! Methods that pair and remove Remote Devices.

use super::{
    ProtocolMethod, DEVICES_APPROVE_JOIN_REQUEST, DEVICES_CANCEL_INVITE, DEVICES_CREATE_INVITE,
    DEVICES_PREVIEW_JOIN_REQUEST, DEVICES_REMOVE,
};
use crate::devices::{
    DevicesApproveJoinRequestParams, DevicesApproveJoinRequestResult, DevicesCancelInviteParams,
    DevicesCancelInviteResult, DevicesCreateInviteParams, DevicesCreateInviteResult,
    DevicesPreviewJoinRequestParams, DevicesPreviewJoinRequestResult, DevicesRemoveParams,
    DevicesRemoveResult,
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
    DevicesPreviewJoinRequest,
    DEVICES_PREVIEW_JOIN_REQUEST,
    DevicesPreviewJoinRequestParams,
    DevicesPreviewJoinRequestResult
);
protocol_method!(
    DevicesApproveJoinRequest,
    DEVICES_APPROVE_JOIN_REQUEST,
    DevicesApproveJoinRequestParams,
    DevicesApproveJoinRequestResult
);
protocol_method!(
    DevicesRemove,
    DEVICES_REMOVE,
    DevicesRemoveParams,
    DevicesRemoveResult
);
