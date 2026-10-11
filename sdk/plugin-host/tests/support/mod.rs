#![allow(non_snake_case)]
use fluent_plugin_host::NativeManifest;
use std::path::{Path, PathBuf};

fn binaryDirectory() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}
pub fn install(runtime: &Path) -> PathBuf {
    fluent_sandbox_fixture::marker();
    let plugin = runtime.join("plugins/sandbox-test");
    std::fs::create_dir_all(&plugin).unwrap();
    let binary = binaryDirectory();
    let candidates = [
        binary.join("fluent_sandbox_fixture.dll"),
        binary.join("deps/fluent_sandbox_fixture.dll"),
    ];
    std::fs::copy(
        candidates
            .iter()
            .find(|path| path.is_file())
            .expect("Fixture DLL missing"),
        plugin.join("provider.dll"),
    )
    .unwrap();
    std::fs::write(plugin.join("plugin.txt"), b"own resource").unwrap();
    let outside = runtime.join("outside.txt");
    std::fs::write(&outside, b"synthetic outside secret").unwrap();
    let manifest = NativeManifest {
        AbiVersion: fluent_plugin_abi::abiVersion,
        Manifest: Default::default(),
        Id: "sandbox-test".into(),
        Library: "provider.dll".into(),
        Options: std::collections::BTreeMap::from([(
            "outsidePath".into(),
            outside.to_string_lossy().into_owned(),
        )]),
    };
    let path = plugin.join("plugin.json");
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    path
}
