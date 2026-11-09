//! Custom WebView protocol registration.
//!
//! Handlers are registered on each WebView builder. Wry invokes them on the
//! native WebView's protocol callback.

use std::sync::Arc;

use anyhow::Result;
use wry::http::{Request, Response};

/// Response returned by a custom protocol handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolResponse {
    /// MIME type such as `text/html` or `application/json`.
    pub mime: String,
    /// Response body bytes.
    pub bytes: Vec<u8>,
    /// HTTP status code.
    pub status: u16,
}

impl ProtocolResponse {
    /// Creates a 200 response with a MIME type and body.
    pub fn new(mime: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            mime: mime.into(),
            bytes: bytes.into(),
            status: 200,
        }
    }

    pub(crate) fn into_wry_response(self) -> Response<std::borrow::Cow<'static, [u8]>> {
        Response::builder()
            .status(self.status)
            .header("Content-Type", self.mime)
            .body(std::borrow::Cow::Owned(self.bytes))
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(500)
                    .body(std::borrow::Cow::Borrowed(
                        &b"invalid protocol response"[..],
                    ))
                    .expect("static protocol fallback response")
            })
    }
}

/// A custom protocol handler.
pub type ProtocolHandler =
    Arc<dyn Fn(&Request<Vec<u8>>) -> Result<ProtocolResponse> + Send + Sync + 'static>;

/// Custom protocol registration used by [`crate::webview::WebViewConfig`].
#[derive(Clone)]
pub struct CustomProtocol {
    /// URL scheme name without `://`.
    pub scheme: String,
    /// Request handler.
    pub handler: ProtocolHandler,
}

impl CustomProtocol {
    /// Creates a custom protocol registration.
    pub fn new<F>(scheme: impl Into<String>, handler: F) -> Self
    where
        F: Fn(&Request<Vec<u8>>) -> Result<ProtocolResponse> + Send + Sync + 'static,
    {
        Self {
            scheme: scheme.into(),
            handler: Arc::new(handler),
        }
    }
}

impl std::fmt::Debug for CustomProtocol {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CustomProtocol")
            .field("scheme", &self.scheme)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_response_sets_mime_and_status() {
        let response = ProtocolResponse::new("text/plain", b"hello".to_vec());
        assert_eq!(response.mime, "text/plain");
        assert_eq!(response.status, 200);
        assert_eq!(response.bytes, b"hello");
    }
}
