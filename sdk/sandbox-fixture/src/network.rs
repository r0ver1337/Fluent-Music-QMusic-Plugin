use fluent_plugin_abi::network::{NetworkRequest, call};
use fluent_plugin_api::{ApiError, Page, ResultData, Track};
use std::{io::Read, time::Duration};

pub fn probe(query: &str) -> Result<ResultData, ApiError> {
    let fail = || ApiError::new("NETWORK", "网络请求失败");
    if let Some(address) = query.strip_prefix("raw:") {
        let address = address.parse().map_err(|_| fail())?;
        match std::net::TcpStream::connect_timeout(&address, Duration::from_millis(400)) {
            Err(_) => {}
            Ok(_) => {
                return Err(ApiError::new(
                    "RAW_ACCESS",
                    "Raw network was not denied by the OS",
                ));
            }
        }
    } else if let Some(url) = query.strip_prefix("hostHeader:") {
        call(NetworkRequest {
            Url: url.into(),
            Method: "GET".into(),
            TimeoutMs: 2000,
            Headers: vec![("Host".into(), "not-declared.example".into())],
            ..Default::default()
        })
        .map_err(|_| fail())?;
    } else {
        let client = fluent_plugin_abi::network::http::Client::builder()
            .timeout(Duration::from_secs(2))
            .user_agent("FluentFixture/2")
            .build()
            .map_err(|_| fail())?;
        let large = query.starts_with("large:");
        let request = if let Some(url) = query.strip_prefix("form:") {
            client
                .post(url)
                .header("User-Agent", "ExplicitFixture/2")
                .form(&[("word", "中文"), ("other", "a+b")])
        } else {
            client.get(query.strip_prefix("large:").unwrap_or(query))
        };
        let mut response = request.send().map_err(|_| fail())?;
        let mut bytes = Vec::new();
        response.read_to_end(&mut bytes).map_err(|_| fail())?;
        let valid = if large {
            bytes.len() == 3 * fluent_plugin_abi::network::chunkBytes + 17
                && bytes
                    .iter()
                    .enumerate()
                    .all(|(index, byte)| *byte == (index % 251) as u8)
        } else {
            bytes == b"fixture-body"
        };
        if !valid {
            return Err(ApiError::new("BODY", "Unexpected fixture body"));
        }
        if response.headers().get_all("set-cookie").iter().count() != 2 {
            return Err(ApiError::new("HEADERS", "Duplicate cookies lost"));
        }
    }
    Ok(ResultData::Tracks(Page {
        items: vec![Track {
            title: "network-checked".into(),
            ..super::track()
        }],
        page: 0,
        has_more: false,
        cursor: None,
    }))
}
