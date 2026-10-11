#![allow(non_snake_case)]
use fluent_plugin_host::PluginProcess;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !(2..=3).contains(&args.len()) {
        return Err("Usage: describePlugins <manifest> <client-directory> [worker-file]".into());
    }
    let manifest = PathBuf::from(&args[0]);
    let runtime = PathBuf::from(&args[1]).canonicalize()?;
    let worker = args
        .get(2)
        .map(|name| runtime.join(name))
        .unwrap_or_else(|| runtime.join("pluginloader.exe"));
    let plugin = PluginProcess::startWithRuntime(&manifest, &runtime, &worker, true)?;
    println!(
        "C ABI handshake passed: id={}, capabilities={}",
        plugin.info.id,
        plugin.info.capabilities.len()
    );
    Ok(())
}
