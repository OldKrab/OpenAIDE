//! Task Navigation as a server-computed view subscription (ADR-0086).

use super::{ProtocolMethod, NAVIGATION_SUBSCRIBE, NAVIGATION_UNSUBSCRIBE, NAVIGATION_UPDATE};
use crate::navigation::{
    NavigationSubscribeResult, NavigationUnsubscribeParams, NavigationUnsubscribeResult,
    NavigationUpdateResult, NavigationViewParams,
};

protocol_method!(
    NavigationSubscribe,
    NAVIGATION_SUBSCRIBE,
    NavigationViewParams,
    NavigationSubscribeResult
);
protocol_method!(
    NavigationUpdate,
    NAVIGATION_UPDATE,
    NavigationViewParams,
    NavigationUpdateResult
);
protocol_method!(
    NavigationUnsubscribe,
    NAVIGATION_UNSUBSCRIBE,
    NavigationUnsubscribeParams,
    NavigationUnsubscribeResult
);
