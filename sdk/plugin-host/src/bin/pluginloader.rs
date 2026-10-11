#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    if let Err(error) = fluent_plugin_host::runWorker() {
        use std::io::Write;
        let response = fluent_plugin_api::Response::new(
            1,
            Err(fluent_plugin_api::ApiError::new(
                "WORKER_START",
                error.to_string(),
            )),
        );
        let message = serde_json::json!({"type":"response", "data":response});
        let _ = writeln!(std::io::stdout(), "{message}");
        std::process::exit(1);
    }
}
