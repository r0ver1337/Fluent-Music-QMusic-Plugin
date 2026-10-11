use fluent_plugin_api::*;
use fluent_plugin_host::PluginProcess;

#[test]
fn host_loads_real_qq_dll_without_requesting_network() {
    let dir = tempfile::tempdir().unwrap();
    let plugin_directory = dir.path().join("plugins/qqmusic");
    std::fs::create_dir_all(&plugin_directory).unwrap();
    std::fs::create_dir(plugin_directory.join(".env")).unwrap();
    let executable = "qqmusic.dll";
    let binary_directory = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let candidates = [
        binary_directory.join("qqmusic.dll"),
        binary_directory.join("deps/qqmusic.dll"),
    ];
    let library = candidates
        .iter()
        .find(|p| p.is_file())
        .expect("QQ DLL missing");
    std::fs::copy(library, plugin_directory.join(executable)).unwrap();
    let manifest = fluent_plugin_host::NativeManifest {
        AbiVersion: fluent_plugin_abi::abiVersion,
        Manifest: Default::default(),
        Id: "qqmusic".into(),
        Library: executable.into(),
        Options: Default::default(),
    };
    let path = plugin_directory.join("plugin.json");
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    for options in [
        Default::default(),
        std::collections::BTreeMap::from([("homepageMode".into(), "flyme".into())]),
        std::collections::BTreeMap::from([("homepageMode".into(), "opi".into())]),
    ] {
        let manifest = fluent_plugin_host::NativeManifest {
            Options: options,
            AbiVersion: manifest.AbiVersion,
            Id: manifest.Id.clone(),
            Library: manifest.Library.clone(),
            Manifest: manifest.Manifest.clone(),
        };
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let process = PluginProcess::startWithRuntime(
            &path,
            dir.path(),
            &binary_directory.join("pluginloader.exe"),
            true,
        )
        .unwrap();
        assert_eq!(process.info.id, "qqmusic");
        assert!(process.info.supports(&Capability::Homepage));
    }
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let mut process = PluginProcess::startWithRuntime(
        &path,
        dir.path(),
        &binary_directory.join("pluginloader.exe"),
        true,
    )
    .unwrap();
    assert_eq!(process.info.id, "qqmusic");
    assert!(process.info.capabilities.contains(&Capability::Resolve));
    assert!(process.info.supports(&Capability::Homepage));
    assert!(process.info.supports(&Capability::PlaylistRecommendations));
    assert!(process.info.supports(&Capability::TrackQualities));
    let ResultData::TrackQualities(catalogue) = process
        .call(Operation::TrackQualities { track: None })
        .unwrap()
    else {
        panic!("quality catalogue")
    };
    assert_eq!(catalogue.Levels.len(), 7);
    assert!(catalogue.Items.is_empty());
    assert_eq!(catalogue.Levels[4].Name, "高品质");
    assert_eq!(catalogue.Levels[4].Priority, 5);
    assert!(process.info.capabilities.contains(&Capability::Homepage));
    assert!(matches!(
        process.call(Operation::Describe).unwrap(),
        ResultData::Description(_)
    ));
    assert!(plugin_directory.join(".env").is_dir());
    assert!(matches!(
        process.call(Operation::Logout).unwrap(),
        ResultData::Ok
    ));
    process.info.capabilities.clear();
    assert_eq!(
        process.call(Operation::Account).err().unwrap().code,
        "UNSUPPORTED"
    );
    process.terminator().terminate();
    assert!(process.call(Operation::Describe).is_err());
    drop(process); // Must terminate and reap the child.
}

#[test]
fn actual_plugin_rejects_wrong_version_and_exits_on_eof() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_fluent-qqmusic-plugin"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let request = Request {
        api_version: 999,
        id: 12,
        operation: Operation::Describe,
    };
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        serde_json::to_string(&request).unwrap()
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let response: Response = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response.id, 12);
    assert_eq!(response.error.unwrap().code, "VERSION");
}

#[test]
fn saved_session_presence_diagnostic_is_opt_in_and_does_not_create_storage() {
    use std::process::{Command, Stdio};
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("private");
    let mut command = Command::new(env!("CARGO_BIN_EXE_fluent-qqmusic-plugin"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("QQMUSIC_") {
            command.env_remove(name);
        }
    }
    let output = command
        .arg("--diagnose-session")
        .current_dir(directory.path())
        .env("QQMUSIC_DATA_DIR", &data)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["saved_session"], false);
    assert_eq!(result["music_id_available"], false);
    assert_eq!(result["network_requests_sent"], false);
    assert!(!data.exists());
}

#[cfg(windows)]
#[test]
fn actual_plugin_opt_out_skips_saved_session_but_logout_deletes_it() {
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Command, Stdio},
    };
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    std::fs::create_dir(&data).unwrap();
    let path = data.join("session.dat");
    std::fs::write(&path, b"malformed synthetic cache, not a credential").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_fluent-qqmusic-plugin"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("QQMUSIC_") {
            command.env_remove(name);
        }
    }
    let mut child = command
        .current_dir(directory.path())
        .env("QQMUSIC_PERSIST_SESSION", "0")
        .env("QQMUSIC_DATA_DIR", &data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    for (id, operation) in [
        Operation::Describe,
        Operation::Account,
        Operation::Logout,
        Operation::Account,
    ]
    .into_iter()
    .enumerate()
    {
        writeln!(
            input,
            "{}",
            serde_json::to_string(&Request {
                api_version: API_VERSION,
                id: id as u64,
                operation
            })
            .unwrap()
        )
        .unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let response: Response = serde_json::from_str(&line).unwrap();
        match id {
            0 => {
                assert!(matches!(response.result, Some(ResultData::Description(_))));
                assert!(path.exists());
                assert!(!data.join("session.lock").exists());
            }
            1 => {
                assert_eq!(response.error.unwrap().code, "LOGIN_REQUIRED");
                assert!(path.exists());
            }
            2 => {
                assert!(matches!(response.result, Some(ResultData::Ok)));
                assert!(!path.exists());
            }
            3 => assert_eq!(response.error.unwrap().code, "LOGIN_REQUIRED"),
            _ => unreachable!(),
        }
        assert!(!line.contains("malformed synthetic cache"));
    }
    drop(input);
    assert!(child.wait().unwrap().success());
}

#[cfg(windows)]
#[test]
fn actual_plugin_logout_removes_cache_even_before_any_restore_or_configuration() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.dat");
    std::fs::write(&path, b"invalid old cache").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_fluent-qqmusic-plugin"))
        .current_dir(directory.path())
        .env("QQMUSIC_PERSIST_SESSION", "1")
        .env("QQMUSIC_DATA_DIR", directory.path())
        .env_remove("QQMUSIC_APP_ID")
        .env_remove("QQMUSIC_APP_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        serde_json::to_string(&Request {
            api_version: API_VERSION,
            id: 7,
            operation: Operation::Logout
        })
        .unwrap()
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let response: Response = serde_json::from_slice(&output.stdout).unwrap();
    assert!(matches!(response.result, Some(ResultData::Ok)));
    assert!(!path.exists());
}
