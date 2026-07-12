use serde::{Deserialize, Serialize};
use ts_rs::TS;

macro_rules! id_type {
    ($name:ident, $brand:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize, TS,
        )]
        #[serde(transparent)]
        #[ts(type = $brand)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_string())
            }
        }
    };
}

id_type!(
    AgentId,
    "string & { readonly __openaideBrand: \"AgentId\" }"
);
id_type!(
    AttachmentId,
    "string & { readonly __openaideBrand: \"AttachmentId\" }"
);
id_type!(
    AttachmentHandleId,
    "string & { readonly __openaideBrand: \"AttachmentHandleId\" }"
);
id_type!(
    AttachmentCandidateId,
    "string & { readonly __openaideBrand: \"AttachmentCandidateId\" }"
);
id_type!(
    AgentConfigOptionId,
    "string & { readonly __openaideBrand: \"AgentConfigOptionId\" }"
);
id_type!(
    ClientInstanceId,
    "string & { readonly __openaideBrand: \"ClientInstanceId\" }"
);
id_type!(
    ClientMutationId,
    "string & { readonly __openaideBrand: \"ClientMutationId\" }"
);
id_type!(
    ClientRequestId,
    "string & { readonly __openaideBrand: \"ClientRequestId\" }"
);
id_type!(
    EventCursor,
    "string & { readonly __openaideBrand: \"EventCursor\" }"
);
id_type!(
    FileBrowserEntryId,
    "string & { readonly __openaideBrand: \"FileBrowserEntryId\" }"
);
id_type!(
    FileBrowserRootId,
    "string & { readonly __openaideBrand: \"FileBrowserRootId\" }"
);
id_type!(
    MessageId,
    "string & { readonly __openaideBrand: \"MessageId\" }"
);
id_type!(
    ProjectId,
    "string & { readonly __openaideBrand: \"ProjectId\" }"
);
id_type!(
    RequestId,
    "string & { readonly __openaideBrand: \"RequestId\" }"
);
id_type!(
    ServerId,
    "string & { readonly __openaideBrand: \"ServerId\" }"
);
id_type!(
    StateRootId,
    "string & { readonly __openaideBrand: \"StateRootId\" }"
);
id_type!(TaskId, "string & { readonly __openaideBrand: \"TaskId\" }");
id_type!(
    TaskListCursor,
    "string & { readonly __openaideBrand: \"TaskListCursor\" }"
);
id_type!(TurnId, "string & { readonly __openaideBrand: \"TurnId\" }");
