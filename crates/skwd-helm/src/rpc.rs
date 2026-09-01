use std::path::Path;

use serde_json::Value;

use super::error::CliError;

pub(super) fn call(socket: &Path, method: &str, params: &Value) -> Result<Value, CliError> {
    let mut connection =
        wall_proto::client::Conn::connect_to(socket).map_err(|_| CliError::Unreachable)?;
    connection.call(method, params).map_err(|error| match error {
        wall_proto::client::CallError::Unreachable => CliError::Unreachable,
        wall_proto::client::CallError::Rpc(info) => CliError::Rpc(info.code, info.message),
    })
}
