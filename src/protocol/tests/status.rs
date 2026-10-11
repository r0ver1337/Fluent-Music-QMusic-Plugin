use super::*;
#[test]
fn app_permission_denial_is_not_login_failure_and_cannot_be_masked_by_code_zero() {
    for data in [
        json!({"ret":10200}),
        json!({"ret":10200,"subRet":0,"code":0,"msg":"raw-token-secret"}),
    ] {
        let get = success(data.clone()).err().unwrap();
        let homepage = homepage_success(data).err().unwrap();
        for error in [&get, &homepage] {
            assert_eq!(error.code, "APP_PERMISSION_DENIED");
            assert!(error.message.contains("OPI") && error.message.contains("ret=10200"));
            assert!(error.message.contains("重新扫码不能解决"));
            assert!(!error.message.contains("raw-token-secret"));
        }
        assert!(homepage.message.contains(HOMEPAGE));
    }
    // Do not assign a root-ret meaning to an unconfirmed business/subRet code.
    assert_eq!(
        homepage_success(json!({"code":10200})).err().unwrap().code,
        "SCHEMA"
    );
    assert_eq!(
        homepage_success(json!({"ret":0,"subRet":10200}))
            .err()
            .unwrap()
            .code,
        "SERVER"
    );
    assert_eq!(
        homepage_success(json!({"ret":-1})).err().unwrap().code,
        "SERVER"
    );
}
#[test]
fn homepage_status_requires_ret_and_checks_all_present_error_fields() {
    for v in [json!({"ret":0}), json!({"code":0,"ret":0,"subRet":0})] {
        assert!(homepage_success(v).is_ok());
    }
    for v in [
        json!({"code":0,"ret":-1}),
        json!({"code":0,"subRet":-1}),
        json!({"ret":0,"code":-1}),
        json!({"code":"0"}),
        json!({"code":0}),
        json!({"ret":0,"subRet":0,"sub_ret":-1}),
        json!({"ret":0,"sub_ret":"0"}),
        json!({}),
        json!({"subRet":0}),
    ] {
        assert!(homepage_success(v).is_err());
    }
}
#[test]
fn ret_and_subret_must_both_succeed() {
    assert!(success(json!({"ret":0})).is_ok());
    assert!(success(json!({"ret":0,"sub_ret":-11})).is_err());
    assert!(success(json!({"ret":"0"})).is_err());
}
