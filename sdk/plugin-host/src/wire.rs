#![allow(non_snake_case)]
use fluent_plugin_abi::files::{FileReply, FileRequest};
use fluent_plugin_abi::network::{NetworkReply, NetworkRequest};
use fluent_plugin_api::{Request, Response};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum HostMessage {
    Request(Request),
    FileReply(FileReply),
    NetworkReply(NetworkReply),
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum WorkerMessage {
    Response(Response),
    FileRequest(FileRequest),
    NetworkRequest(NetworkRequest),
}
pub fn encode<T: Serialize>(value: &T) -> std::io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() >= fluent_plugin_api::MAX_MESSAGE_BYTES {
        return Err(std::io::Error::other("Internal IPC frame too large"));
    }
    bytes.push(b'\n');
    Ok(bytes)
}
