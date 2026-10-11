use md5::{Digest, Md5};
use std::collections::BTreeMap;

pub fn hash(text: &str) -> String {
    format!("{:x}", Md5::digest(text.as_bytes()))
}
pub fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
pub fn signed(mut parameters: BTreeMap<String, String>, key: &str) -> BTreeMap<String, String> {
    parameters.remove("sign");
    parameters.remove("tech_version");
    let timestamp = parameters.get("timestamp").cloned().unwrap_or_default();
    parameters.insert(
        "tech_version".into(),
        format!("sdk|{}", hash(&hash(&format!("sdk_{key}_{timestamp}")))),
    );
    let plain = parameters
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    parameters.insert("sign".into(), hash(&format!("{plain}_{key}")));
    parameters
}
