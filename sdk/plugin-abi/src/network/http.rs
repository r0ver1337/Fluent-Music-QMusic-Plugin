use super::{NetworkRequest, call, chunkBytes, maxRequestBytes};
use reqwest::{
    IntoUrl, Method, StatusCode,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde::Serialize;
use std::{
    io::{self, Read},
    time::Duration,
};

#[derive(Clone)]
pub struct Client {
    Encoder: reqwest::blocking::Client,
    Timeout: Duration,
    UserAgent: Option<HeaderValue>,
}
pub struct ClientBuilder {
    Encoder: reqwest::blocking::ClientBuilder,
    Timeout: Duration,
    UserAgent: Option<HeaderValue>,
}
pub struct RequestBuilder {
    Encoder: reqwest::blocking::RequestBuilder,
    Timeout: Duration,
    UserAgent: Option<HeaderValue>,
}
pub struct Response {
    Handle: u64,
    Status: StatusCode,
    Headers: HeaderMap,
}
impl Client {
    pub fn builder() -> ClientBuilder {
        ClientBuilder {
            Encoder: reqwest::blocking::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none()),
            Timeout: Duration::from_secs(15),
            UserAgent: None,
        }
    }
    pub fn request(&self, method: Method, url: impl IntoUrl) -> RequestBuilder {
        RequestBuilder {
            Encoder: self.Encoder.request(method, url),
            Timeout: self.Timeout,
            UserAgent: self.UserAgent.clone(),
        }
    }
    pub fn get(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::GET, url)
    }
    pub fn post(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::POST, url)
    }
}
impl ClientBuilder {
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.Timeout = timeout;
        self
    }
    pub fn connect_timeout(self, _: Duration) -> Self {
        self
    }
    pub fn redirect(self, _: reqwest::redirect::Policy) -> Self {
        self
    }
    pub fn no_proxy(self) -> Self {
        self
    }
    pub fn user_agent(mut self, value: impl AsRef<str>) -> Self {
        self.UserAgent = HeaderValue::from_str(value.as_ref()).ok();
        self.Encoder = self.Encoder.user_agent(value.as_ref());
        self
    }
    pub fn build(self) -> Result<Client, reqwest::Error> {
        Ok(Client {
            Encoder: self.Encoder.build()?,
            Timeout: self.Timeout,
            UserAgent: self.UserAgent,
        })
    }
}
impl RequestBuilder {
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.Timeout = timeout;
        self
    }
    pub fn header<K, V>(mut self, name: K, value: V) -> Self
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http_types::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http_types::Error>,
    {
        self.Encoder = self.Encoder.header(name, value);
        self
    }
    pub fn body(mut self, body: impl Into<reqwest::blocking::Body>) -> Self {
        self.Encoder = self.Encoder.body(body);
        self
    }
    pub fn json<T: Serialize + ?Sized>(mut self, body: &T) -> Self {
        self.Encoder = self.Encoder.json(body);
        self
    }
    pub fn form<T: Serialize + ?Sized>(mut self, body: &T) -> Self {
        self.Encoder = self.Encoder.form(body);
        self
    }
    pub fn query<T: Serialize + ?Sized>(mut self, query: &T) -> Self {
        self.Encoder = self.Encoder.query(query);
        self
    }
    pub fn send(self) -> io::Result<Response> {
        let mut request = self
            .Encoder
            .build()
            .map_err(|_| io::Error::other("Invalid HTTP request"))?;
        if !request.headers().contains_key(reqwest::header::USER_AGENT)
            && let Some(value) = self.UserAgent
        {
            request
                .headers_mut()
                .insert(reqwest::header::USER_AGENT, value);
        }
        let body = match request.body() {
            None => vec![],
            Some(body) => body
                .as_bytes()
                .ok_or_else(|| io::Error::other("Streaming request bodies are unsupported"))?
                .to_vec(),
        };
        if body.len() > maxRequestBytes {
            return Err(io::Error::other("HTTP body too large"));
        }
        let headers = request
            .headers()
            .iter()
            .map(|(name, value)| {
                Ok((
                    name.to_string(),
                    value
                        .to_str()
                        .map_err(|_| io::Error::other("Invalid HTTP header"))?
                        .to_owned(),
                ))
            })
            .collect::<io::Result<_>>()?;
        let reply = call(NetworkRequest {
            Method: request.method().to_string(),
            Url: request.url().to_string(),
            Headers: headers,
            Body: body,
            TimeoutMs: self.Timeout.as_millis().min(180000) as u64,
            ..Default::default()
        })?;
        let result = (|| {
            let mut headers = HeaderMap::new();
            for (name, value) in reply.Headers {
                headers.append(
                    HeaderName::from_bytes(name.as_bytes()).map_err(io::Error::other)?,
                    HeaderValue::from_str(&value).map_err(io::Error::other)?,
                );
            }
            Ok(Response {
                Handle: reply.Handle,
                Status: StatusCode::from_u16(reply.Status).map_err(io::Error::other)?,
                Headers: headers,
            })
        })();
        if result.is_err() {
            let _ = call(NetworkRequest {
                Operation: 2,
                Handle: reply.Handle,
                ..Default::default()
            });
        }
        result
    }
}
impl Response {
    pub fn status(&self) -> StatusCode {
        self.Status
    }
    pub fn headers(&self) -> &HeaderMap {
        &self.Headers
    }
    pub fn content_length(&self) -> Option<u64> {
        self.Headers
            .get("content-length")?
            .to_str()
            .ok()?
            .parse()
            .ok()
    }
}
impl Read for Response {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let reply = call(NetworkRequest {
            Operation: 1,
            Handle: self.Handle,
            Length: bytes.len().min(chunkBytes) as u64,
            ..Default::default()
        })?;
        if reply.Body.len() > bytes.len().min(chunkBytes) {
            return Err(io::Error::other("Invalid HTTP read"));
        }
        bytes[..reply.Body.len()].copy_from_slice(&reply.Body);
        Ok(reply.Body.len())
    }
}
impl Drop for Response {
    fn drop(&mut self) {
        let _ = call(NetworkRequest {
            Operation: 2,
            Handle: self.Handle,
            ..Default::default()
        });
    }
}
