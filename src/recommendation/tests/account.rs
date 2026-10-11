use super::*;
fn synthetic_session() -> Session {
    Session {
        open_id: "synthetic-openid".into(),
        access: "synthetic-access".into(),
        refresh: "synthetic-refresh".into(),
        expires: u64::MAX,
        music_id: Some("999999".into()),
    }
}
#[test]
fn openid_branch_is_not_confused_with_qq_musicid_or_appid() {
    let mut c = RecommendationClient::new("synthetic-device".into()).unwrap();
    c.set_open_id_account("synthetic-app", &synthetic_session())
        .unwrap();
    let comm = c.comm(false);
    assert_eq!(comm["openAppID"], "synthetic-app");
    assert_eq!(comm["uin"], "synthetic-openid");
    assert_eq!(comm["authst"], "synthetic-access");
    assert!(!comm.contains_key("qq") && !comm.contains_key("tmeLoginType"));
    let early = c.comm(true);
    assert!(!early.contains_key("authst") && !early.contains_key("uin"));
    assert_eq!(c.feed(Instant::now()).err().unwrap().code, "SESSION");
}
#[test]
fn account_verification_is_invalidated_on_rotation_account_change_and_clear() {
    let mut c = RecommendationClient::new("synthetic-device".into()).unwrap();
    let mut s = synthetic_session();
    c.set_open_id_account("synthetic-app", &s).unwrap();
    c.account_verified_until = Some(Instant::now() + Duration::from_secs(60));
    c.set_open_id_account("synthetic-app", &s).unwrap();
    assert!(c.account_verified_until.is_some());
    s.access = "new-access".into();
    c.set_open_id_account("synthetic-app", &s).unwrap();
    assert!(c.account_verified_until.is_none());
    c.account_verified_until = Some(Instant::now());
    s.open_id = "other-openid".into();
    c.set_open_id_account("synthetic-app", &s).unwrap();
    assert!(c.account_verified_until.is_none());
    c.clear();
    assert!(c.account.is_none() && c.account_verified_until.is_none() && c.base.is_none());
}
