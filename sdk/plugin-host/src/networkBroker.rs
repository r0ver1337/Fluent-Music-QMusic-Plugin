use crate::networkPolicy::{NetworkPolicy, failed};
use fluent_plugin_abi::network::*;
use std::{
    collections::HashMap,
    io::Read,
    time::{Duration, Instant},
};

pub struct NetworkBroker {
    Policy: NetworkPolicy,
    Streams: HashMap<u64, Stream>,
    Next: u64,
}
struct Stream {
    Response: reqwest::blocking::Response,
    Expires: Instant,
    Remaining: u64,
}
impl NetworkBroker {
    pub fn new(policy: NetworkPolicy) -> Self {
        Self {
            Policy: policy,
            Streams: HashMap::new(),
            Next: 0,
        }
    }
    pub fn call(&mut self, request: NetworkRequest) -> NetworkReply {
        self.Streams
            .retain(|_, stream| stream.Expires > Instant::now());
        self.perform(request).unwrap_or_else(|_| NetworkReply {
            Failed: true,
            ..Default::default()
        })
    }
    fn perform(
        &mut self,
        request: NetworkRequest,
    ) -> Result<NetworkReply, fluent_plugin_api::ApiError> {
        match request.Operation {
            0 => self.open(request),
            1 if request.Length > 0 && request.Length <= chunkBytes as u64 => {
                let stream = self.Streams.get_mut(&request.Handle).ok_or_else(failed)?;
                let mut bytes = vec![0; request.Length as usize];
                let count = match stream.Response.read(&mut bytes) {
                    Ok(count) if count as u64 <= stream.Remaining => count,
                    _ => {
                        self.Streams.remove(&request.Handle);
                        return Err(failed());
                    }
                };
                stream.Remaining -= count as u64;
                bytes.truncate(count);
                Ok(NetworkReply {
                    Body: bytes,
                    ..Default::default()
                })
            }
            2 => {
                self.Streams.remove(&request.Handle);
                Ok(NetworkReply::default())
            }
            _ => Err(failed()),
        }
    }
    fn open(
        &mut self,
        request: NetworkRequest,
    ) -> Result<NetworkReply, fluent_plugin_api::ApiError> {
        let target = self.Policy.check(&request.Url)?;
        if self.Streams.len() >= 8
            || request.Body.len() > maxRequestBytes
            || request.TimeoutMs == 0
            || request.Headers.len() > 128
            || !matches!(
                request.Method.as_str(),
                "GET" | "POST" | "HEAD" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
            )
        {
            return Err(failed());
        }
        let timeout = Duration::from_millis(request.TimeoutMs.min(180000));
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(timeout.min(Duration::from_secs(5)))
            .timeout(timeout)
            .build()
            .map_err(|_| failed())?;
        let method =
            reqwest::Method::from_bytes(request.Method.as_bytes()).map_err(|_| failed())?;
        let mut builder = client.request(method, target);
        let mut headerBytes = 0;
        for (name, value) in request.Headers {
            headerBytes += name.len() + value.len();
            if headerBytes > 65536
                || matches!(
                    name.to_ascii_lowercase().as_str(),
                    "host"
                        | "connection"
                        | "upgrade"
                        | "proxy-authorization"
                        | "proxy-connection"
                        | "transfer-encoding"
                        | "content-length"
                )
            {
                return Err(failed());
            }
            builder = builder.header(name, value);
        }
        let response = builder.body(request.Body).send().map_err(|_| failed())?;
        if response.status().is_redirection()
            || response
                .content_length()
                .is_some_and(|length| length > 512 * 1024 * 1024)
        {
            return Err(failed());
        }
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                Ok((
                    name.to_string(),
                    value.to_str().map_err(|_| failed())?.to_owned(),
                ))
            })
            .collect::<Result<Vec<_>, fluent_plugin_api::ApiError>>()?;
        if headers.len() > 128
            || headers
                .iter()
                .map(|(name, value)| name.len() + value.len())
                .sum::<usize>()
                > 65536
        {
            return Err(failed());
        }
        self.Next = self.Next.checked_add(1).ok_or_else(failed)?;
        let reply = NetworkReply {
            Handle: self.Next,
            Status: response.status().as_u16(),
            Headers: headers,
            ..Default::default()
        };
        self.Streams.insert(
            self.Next,
            Stream {
                Response: response,
                Expires: Instant::now() + timeout,
                Remaining: 512 * 1024 * 1024,
            },
        );
        Ok(reply)
    }
}
