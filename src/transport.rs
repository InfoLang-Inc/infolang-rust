use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, USER_AGENT};
use reqwest::{Client as HttpClient, Method};
use serde_json::Value;

use crate::auth::AuthProvider;
use crate::error::{api_error_from_response, ConfigError};
use crate::types::Metering;
use crate::{Error, VERSION};

const RETRY_STATUSES: &[u16] = &[429, 500, 502, 503, 504];

/// One file for a multipart ingest (`files` part).
pub(crate) struct FilePart {
    pub name: String,
    pub content: Vec<u8>,
}

/// Request body variants the transport can send (rebuilt per retry attempt).
pub(crate) enum Body {
    None,
    Json(Value),
    /// Pre-encoded bytes sent verbatim with an explicit content type
    /// (zip archives for ingest).
    Raw {
        bytes: Vec<u8>,
        content_type: String,
    },
    /// Repeated `files` parts; reqwest sets the multipart boundary itself,
    /// so no Content-Type is set manually.
    Multipart(Vec<FilePart>),
}

pub(crate) struct Transport {
    pub base_url: String,
    pub http: HttpClient,
    pub auth: Box<dyn AuthProvider>,
    pub user_agent: String,
    pub max_retries: usize,
    pub backoff_base: Duration,
    pub backoff_cap: Duration,
}

pub(crate) struct Response {
    pub data: Value,
    pub metering: Option<Metering>,
}

impl Transport {
    pub async fn do_json(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Response, Error> {
        let body = match body {
            Some(value) => Body::Json(value),
            None => Body::None,
        };
        self.request(method, path, body).await
    }

    pub async fn request(&self, method: Method, path: &str, body: Body) -> Result<Response, Error> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let mut last_err: Option<reqwest::Error> = None;

        for attempt in 0..=self.max_retries {
            let mut request = self.http.request(method.clone(), &url);
            request = self.apply_headers(request);
            match &body {
                Body::None => {}
                Body::Json(payload) => request = request.json(payload),
                Body::Raw {
                    bytes,
                    content_type,
                } => {
                    request = request
                        .header(CONTENT_TYPE, content_type.as_str())
                        .body(bytes.clone());
                }
                Body::Multipart(parts) => {
                    let mut form = reqwest::multipart::Form::new();
                    for part in parts {
                        let piece = reqwest::multipart::Part::bytes(part.content.clone())
                            .file_name(part.name.clone())
                            .mime_str("text/plain")
                            .expect("text/plain is a valid mime type");
                        form = form.part("files", piece);
                    }
                    request = request.multipart(form);
                }
            }

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if RETRY_STATUSES.contains(&status.as_u16()) && attempt < self.max_retries {
                        let retry_after = parse_retry_after(response.headers());
                        let _ = response.bytes().await;
                        tokio::time::sleep(self.delay(attempt, retry_after)).await;
                        continue;
                    }
                    return self.finish(response).await;
                }
                Err(err) => {
                    if err.is_timeout() || err.is_connect() || err.is_request() {
                        if attempt >= self.max_retries {
                            return Err(Error::Connection { source: err });
                        }
                        last_err = Some(err);
                        tokio::time::sleep(self.delay(attempt, 0.0)).await;
                        continue;
                    }
                    return Err(Error::Connection { source: err });
                }
            }
        }

        Err(Error::Connection {
            source: last_err.expect("retry loop should set last_err"),
        })
    }

    fn apply_headers(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut headers = HeaderMap::new();
        if let Ok(value) = HeaderValue::from_str(&self.user_agent) {
            headers.insert(USER_AGENT, value);
        }
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        self.auth.apply(&mut headers);
        request.headers(headers)
    }

    async fn finish(&self, response: reqwest::Response) -> Result<Response, Error> {
        let status = response.status();
        let headers = response.headers().clone();
        let retry_after = parse_retry_after(&headers);
        let metering = parse_metering(&headers);
        let raw = response
            .bytes()
            .await
            .map_err(|source| Error::Connection { source })?;
        let data = decode_body(&raw);

        if status.is_success() {
            return Ok(Response {
                data,
                metering: Some(metering),
            });
        }

        Err(Error::Api(api_error_from_response(
            status.as_u16(),
            data,
            metering.request_id.clone(),
            retry_after,
        )))
    }

    fn delay(&self, attempt: usize, retry_after: f64) -> Duration {
        if retry_after > 0.0 {
            return Duration::from_secs_f64(retry_after);
        }
        let window = self
            .backoff_base
            .as_secs_f64()
            .mul_add(2f64.powi(attempt as i32), 0.0);
        let capped = window.min(self.backoff_cap.as_secs_f64());
        Duration::from_secs_f64(rand_jitter() * capped)
    }
}

fn decode_body(raw: &[u8]) -> Value {
    if raw.iter().all(|b| b.is_ascii_whitespace()) {
        return Value::Null;
    }
    serde_json::from_slice(raw)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(raw).into_owned()))
}

fn parse_retry_after(headers: &HeaderMap) -> f64 {
    headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn parse_metering(headers: &HeaderMap) -> Metering {
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    Metering {
        tokens_saved: parse_header_i64(headers, "x-infolang-tokens-saved"),
        chunks_used: parse_header_i64(headers, "x-infolang-chunks-used"),
        repo_coverage: headers
            .get("x-infolang-repo-coverage")
            .and_then(|value| value.to_str().ok())
            .and_then(|raw| raw.parse::<f64>().ok()),
        overage: parse_header_i64(headers, "x-infolang-overage"),
        request_id,
    }
}

fn parse_header_i64(headers: &HeaderMap, name: &str) -> Option<i64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.parse::<i64>().ok())
}

fn rand_jitter() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos as f64 % 10_000.0) / 10_000.0
}

pub(crate) fn build_http_client(timeout: Duration) -> Result<HttpClient, ConfigError> {
    HttpClient::builder()
        .timeout(timeout)
        .build()
        .map_err(|err| ConfigError {
            message: format!("failed to build HTTP client: {err}"),
        })
}

pub(crate) fn default_user_agent() -> String {
    format!("infolang-rust/{VERSION}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_body_parses_json() {
        let value = decode_body(br#"{"ok":true}"#);
        assert_eq!(value["ok"], true);
    }

    #[test]
    fn parse_retry_after_reads_header() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("2.5"));
        assert_eq!(parse_retry_after(&headers), 2.5);
    }

    #[test]
    fn parse_metering_reads_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", HeaderValue::from_static("req-1"));
        headers.insert("x-infolang-tokens-saved", HeaderValue::from_static("12"));
        headers.insert("x-infolang-chunks-used", HeaderValue::from_static("3"));
        headers.insert("x-infolang-repo-coverage", HeaderValue::from_static("0.5"));
        headers.insert("x-infolang-overage", HeaderValue::from_static("7"));
        let metering = parse_metering(&headers);
        assert_eq!(metering.request_id, "req-1");
        assert_eq!(metering.tokens_saved, Some(12));
        assert_eq!(metering.chunks_used, Some(3));
        assert_eq!(metering.repo_coverage, Some(0.5));
        assert_eq!(metering.overage, Some(7));
    }

    #[test]
    fn decode_body_handles_empty() {
        assert_eq!(decode_body(b"   "), Value::Null);
    }
}
