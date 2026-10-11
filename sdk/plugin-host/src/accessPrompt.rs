#![allow(non_snake_case, non_upper_case_globals)]
use std::sync::{Arc, Mutex};
#[derive(Clone)]
pub struct AccessPrompt {
    pub PluginId: String,
    pub Directory: bool,
    pub Writable: bool,
    pub Purpose: String,
}
pub type AccessApproval = Arc<dyn Fn(AccessPrompt) -> bool + Send + Sync>;
static approval: Mutex<Option<AccessApproval>> = Mutex::new(None);
pub fn setAccessApproval(handler: Option<AccessApproval>) {
    if let Ok(mut current) = approval.lock() {
        *current = handler;
    }
}
pub(crate) fn requestApproval(prompt: AccessPrompt) -> bool {
    let handler = approval.lock().ok().and_then(|h| h.clone());
    handler.is_some_and(|h| h(prompt))
}
