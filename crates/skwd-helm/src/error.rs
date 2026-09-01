pub(super) const RPC_UNKNOWN_METHOD: i32 = -32601;
pub(super) const RPC_INVALID_PARAMS: i32 = -32602;

pub(super) enum CliError {
    Unreachable,
    NotFound(String),
    BadArgs(String),
    Rpc(i32, String),
}

impl CliError {
    pub(super) fn local(message: impl Into<String>) -> Self {
        Self::Rpc(0, message.into())
    }

    pub(super) fn code(&self) -> i32 {
        match self {
            Self::NotFound(_) => 2,
            Self::Unreachable => 3,
            Self::BadArgs(_) => 4,
            Self::Rpc(RPC_UNKNOWN_METHOD, _) => 5,
            Self::Rpc(RPC_INVALID_PARAMS, _) => 6,
            Self::Rpc(_, _) => 1,
        }
    }

    pub(super) fn message(&self) -> String {
        match self {
            Self::Unreachable => "skwd-walld is not reachable (is the daemon running?)".into(),
            Self::NotFound(message) | Self::BadArgs(message) | Self::Rpc(_, message) => {
                message.clone()
            }
        }
    }
}
