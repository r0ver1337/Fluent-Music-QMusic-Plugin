use super::*;

#[test]
fn login_poll_interval_returns_the_last_state_without_sending_duplicate_requests() {
    let server = discovery::testing::Server::new(vec![
        ("200 OK", serde_json::to_vec(&json!({"ret":0,"auth_code":"synthetic-qr","sdk_qr_code":"https://y.qq.com/synthetic-qr"})).unwrap(), ""),
        ("200 OK", serde_json::to_vec(&json!({"ret":-19,"subRet":0})).unwrap(), ""),
        ("200 OK", serde_json::to_vec(&json!({"ret":-10,"subRet":0})).unwrap(), ""),
    ]);
    let mut provider = authenticated_provider(&server.base);
    provider.session = None;
    let ResultData::LoginChallenge(challenge) = provider.handle(Operation::LoginBegin).unwrap()
    else {
        panic!("challenge")
    };
    assert_eq!(challenge.expires_in_seconds, 180);
    let operation = Operation::LoginPoll {
        challenge_id: challenge.challenge_id.clone(),
    };
    assert!(matches!(
        provider.handle(operation.clone()).unwrap(),
        ResultData::LoginState(LoginState::Scanned)
    ));
    assert!(provider.pending.as_ref().unwrap().next_poll > Instant::now());
    assert!(matches!(
        provider.handle(operation.clone()).unwrap(),
        ResultData::LoginState(LoginState::Scanned)
    ));
    assert_eq!(
        provider
            .handle(Operation::LoginPoll {
                challenge_id: "wrong-challenge".into()
            })
            .err()
            .unwrap()
            .code,
        "LOGIN"
    );
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    provider.pending.as_mut().unwrap().next_poll = Instant::now();
    assert!(matches!(
        provider.handle(operation).unwrap(),
        ResultData::LoginState(LoginState::Expired)
    ));
    assert!(provider.pending.is_none());
    server.thread.join().unwrap();
    assert_eq!(server.requests.lock().unwrap().len(), 3);
}

#[cfg(windows)]
#[test]
fn verified_login_is_saved_before_authorized_and_reused_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut values = login_replies(
        json!({"ret":0,"user_info":{"nickname":"合成账号","headimgurl":"http://thirdqq.qlogo.cn/avatar.png"}}),
    );
    values[2] = json!({"ret":0,"encryptString":encrypted_session_with_music_id("synthetic-open-id","login-access","login-refresh",now()+3600,Some(json!("login-music-secret")))});
    values.push(json!({"ret":0,"user_info":{"nickname":"合成账号","headimgurl":"http://thirdqq.qlogo.cn/avatar.png"}}));
    let server = replies(values);
    let mut p = authenticated_provider(&server.base);
    p.session = None;
    p.store = Some(SessionStore::at(dir.path().into()));
    assert!(matches!(
        finish_login(&mut p).unwrap(),
        ResultData::LoginState(LoginState::Authorized(a)) if a.display_name == "合成账号"
            && a.avatar_url.as_deref() == Some("https://thirdqq.qlogo.cn/avatar.png")
    ));
    assert!(p.store.as_ref().unwrap().exists().unwrap());
    let generation = p.generation.clone();
    drop(p); // Ordinary shutdown must NOT erase authorization.
    let mut restarted = authenticated_provider(&server.base);
    restarted.session = None;
    restarted.client.as_mut().unwrap().config.explicit_device_id = false;
    restarted.client.as_mut().unwrap().config.device_id = "new-generated-device".into();
    restarted.store = Some(SessionStore::at(dir.path().into()));
    let reply = restarted.handle(Operation::Account).unwrap();
    assert!(
        matches!(&reply,ResultData::Account(a) if a.display_name=="合成账号"
        && a.avatar_url.as_deref() == Some("https://thirdqq.qlogo.cn/avatar.png"))
    );
    assert_eq!(restarted.generation, generation);
    assert_eq!(
        restarted.client.as_ref().unwrap().config.device_id,
        "-device-with-hyphens"
    );
    assert_eq!(restarted.session.as_ref().unwrap().access, "login-access");
    assert_eq!(
        restarted.session.as_ref().unwrap().music_id.as_deref(),
        Some("login-music-secret")
    );
    assert!(restarted.tokens.is_empty() && restarted.updates.is_empty());
    assert!(
        !serde_json::to_string(&reply)
            .unwrap()
            .contains("login-access")
    );
    assert!(
        !serde_json::to_string(&reply)
            .unwrap()
            .contains("login-music-secret")
    );
    server.thread.join().unwrap();
    let requests = server.requests.lock().unwrap();
    assert!(requests[4].contains(ACCOUNT) && requests[4].contains("login-access"));
    assert!(
        !requests[4].contains("login-refresh") && !requests[4].contains("new-generated-device")
    );
    drop(requests);
    restarted.handle(Operation::Logout).unwrap();
    assert!(!dir.path().join("session.dat").exists());
    assert_eq!(
        restarted.handle(Operation::Account).err().unwrap().code,
        "LOGIN_REQUIRED"
    );
}
#[cfg(windows)]
#[test]
fn fresh_qr_login_does_not_inherit_previous_accounts_music_id() {
    let dir = tempfile::tempdir().unwrap();
    let server = replies(login_replies(
        json!({"ret":0,"user_info":{"nickname":"新账号"}}),
    ));
    let mut p = authenticated_provider(&server.base);
    p.session.as_mut().unwrap().open_id = "previous-open-id".into();
    p.session.as_mut().unwrap().music_id = Some("previous-music-secret".into());
    p.store = Some(SessionStore::at(dir.path().into()));
    p.persist_session().unwrap();
    let previous = p.generation.clone();
    assert!(matches!(
        finish_login(&mut p).unwrap(),
        ResultData::LoginState(LoginState::Authorized(_))
    ));
    assert!(p.session.as_ref().unwrap().music_id.is_none());
    assert_ne!(p.generation, previous);
    assert!(
        p.store
            .as_ref()
            .unwrap()
            .load(&p.client.as_ref().unwrap().config)
            .unwrap()
            .unwrap()
            .session
            .music_id
            .is_none()
    );
    server.thread.join().unwrap();
}
#[cfg(windows)]
#[test]
fn failed_login_validation_or_save_never_publishes_authorized_or_replaces_old_session() {
    for blocked_file in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let account = if blocked_file {
            json!({"ret":0,"user_info":{"nickname":"new"}})
        } else {
            json!({"ret":-1})
        };
        let server = replies(login_replies(account));
        let mut p = authenticated_provider(&server.base);
        let root = dir.path().join("private");
        p.store = Some(SessionStore::at(root.clone()));
        p.persist_session().unwrap();
        let previous = p.generation.clone();
        let path = root.join("session.dat");
        let bytes = std::fs::read(&path).unwrap();
        use std::os::windows::fs::OpenOptionsExt;
        let held = blocked_file.then(|| {
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&path)
                .unwrap()
        });
        let error = finish_login(&mut p).err().unwrap();
        assert_eq!(
            error.code,
            if blocked_file {
                "SESSION_STORAGE"
            } else {
                "SERVER"
            }
        );
        assert_eq!(p.session.as_ref().unwrap().access, "synthetic-access");
        assert_eq!(p.generation, previous);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        drop(held);
        server.thread.join().unwrap();
    }
}
