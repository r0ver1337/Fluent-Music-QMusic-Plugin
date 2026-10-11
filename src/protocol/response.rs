use super::{HOMEPAGE, fail};
use fluent_plugin_api::ApiError;
use serde_json::Value;

pub fn status(data: &Value) -> Result<(i64, i64), ApiError> {
    let ret = data
        .get("ret")
        .and_then(Value::as_i64)
        .ok_or_else(|| fail("SCHEMA", "响应 ret 不是整数"))?;
    let subret = match data.get("subRet").or_else(|| data.get("sub_ret")) {
        Some(v) => v
            .as_i64()
            .ok_or_else(|| fail("SCHEMA", "响应 subRet 不是整数"))?,
        None => 0,
    };
    Ok((ret, subret))
}
pub fn success(data: Value) -> Result<Value, ApiError> {
    let (ret, subret) = status(&data)?;
    if ret == 10200 {
        return Err(application_permission_error("当前接口"));
    }
    if ret != 0 || subret != 0 {
        return Err(ApiError::new(
            "SERVER",
            format!("服务器拒绝请求：ret={ret}, subRet={subret}"),
        ));
    }
    Ok(data)
}

fn application_permission_error(interface: &str) -> ApiError {
    // Confirmed in NetworkErrorCodeUtil.m35646a and its Smali 0x27d8 branch.
    // This is an application grant, not an expired user token or personal membership.
    ApiError::new(
        "APP_PERMISSION_DENIED",
        format!(
            "{interface}的 OPI 请求被应用权限拒绝（ret=10200）；应核对接口/应用配置，重新扫码不能解决"
        ),
    )
}

/// Unlike OpenQQMusic's empty-list-on-error handling, require ret and reject
/// every explicit failure. Keep authenticated parsing separate from public CGI.
pub(super) fn homepage_success(data: Value) -> Result<Value, ApiError> {
    if data.get("ret").and_then(Value::as_i64) == Some(10200) {
        return Err(application_permission_error(HOMEPAGE));
    }
    let data = success(data)?;
    for key in ["subRet", "sub_ret", "code"] {
        if let Some(value) = data.get(key) {
            let n = value
                .as_i64()
                .ok_or_else(|| fail("SCHEMA", "首页推荐状态不是整数"))?;
            if n != 0 {
                return Err(ApiError::new(
                    "SERVER",
                    format!("首页推荐被拒绝：{key}={n}"),
                ));
            }
        }
    }
    Ok(data)
}
pub fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::new("SCHEMA", format!("响应缺少字符串字段 {key}")))
}
pub fn unsigned(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.trim().parse().ok())
}
