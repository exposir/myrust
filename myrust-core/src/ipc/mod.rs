//! Typed IPC between Rust handlers and the JavaScript bridge.
//!
//! The bridge exposes `window.__myrust_ipc__` with `invoke` and `on`.
//! Requests are encoded as JSON and routed by channel name.

/// TypeScript declaration generation for registered IPC channels.
pub mod gen;

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use serde_repr::{Deserialize_repr, Serialize_repr};
use thiserror::Error;

/// JavaScript bridge installed before every WebView page runs.
pub(crate) const BRIDGE_SCRIPT: &str = r#"
(() => {
  if (window.__myrust_ipc__) return;

  const listeners = new Map();
  const pending = new Map();
  let nextId = 1;

  function emit(channel, payload) {
    const handlers = listeners.get(channel);
    if (!handlers) return;
    for (const handler of [...handlers]) {
      try {
        handler(payload);
      } catch (error) {
        console.error(`myrust IPC listener failed for ${channel}:`, error);
      }
    }
  }

  window.__myrust_ipc__ = {
    invoke(channel, payload) {
      return new Promise((resolve, reject) => {
        const id = nextId++;
        pending.set(id, { resolve, reject });
        const message = { id, channel };
        if (payload !== undefined) message.payload = payload;

        try {
          window.ipc.postMessage(JSON.stringify(message));
        } catch (error) {
          pending.delete(id);
          reject(error);
        }
      });
    },

    on(channel, listener) {
      if (typeof listener !== "function") {
        throw new TypeError("IPC listener must be a function");
      }
      const handlers = listeners.get(channel) || new Set();
      handlers.add(listener);
      listeners.set(channel, handlers);
      return () => {
        handlers.delete(listener);
        if (handlers.size === 0) listeners.delete(channel);
      };
    },

    __resolve(message) {
      const request = pending.get(message.id);
      if (!request) return;
      pending.delete(message.id);

      if (message.ok) {
        request.resolve(message.result);
        return;
      }

      const error = new Error(message.error?.message || "IPC request failed");
      error.code = message.error?.code;
      request.reject(error);
    },

    __emit(channel, payload) {
      emit(channel, payload);
    }
  };
})();
"#;

/// A request sent from JavaScript to Rust.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcRequest {
    /// Correlation id chosen by the JavaScript bridge.
    pub id: u64,
    /// Registered Rust handler channel.
    pub channel: String,
    /// JSON payload supplied by the caller.
    #[serde(default)]
    pub payload: Value,
}

/// A response returned from Rust to JavaScript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcReply {
    /// Correlation id copied from the request.
    pub id: u64,
    /// Whether the handler completed successfully.
    pub ok: bool,
    /// Serialized handler value for successful calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Structured failure for unsuccessful calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<IpcError>,
}

impl IpcReply {
    /// Creates a successful reply containing the handler result.
    pub fn success(id: u64, result: Value) -> Self {
        Self {
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    /// Creates a failed reply with a structured error.
    pub fn failure(id: u64, error: IpcError) -> Self {
        Self {
            id,
            ok: false,
            result: None,
            error: Some(error),
        }
    }
}

/// Stable numeric IPC error code shared with TypeScript.
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr)]
#[repr(i32)]
pub enum IpcErrorCode {
    /// The requested channel has no registered handler.
    UnknownChannel = 404,
    /// The payload could not be decoded into the Rust request type.
    InvalidPayload = 400,
    /// The channel name was registered more than once.
    DuplicateChannel = 409,
    /// The handler returned a domain error.
    HandlerFailed = 500,
    /// A Rust response could not be serialized to JSON.
    SerializationFailed = 502,
}

/// Structured error crossing the IPC boundary.
#[derive(Debug, Clone, Serialize, Deserialize, Error)]
#[error("{message}")]
pub struct IpcError {
    /// Stable numeric error code exposed to TypeScript.
    pub code: IpcErrorCode,
    /// Human-readable failure description.
    pub message: String,
}

impl IpcError {
    /// Creates an unknown-channel error.
    pub fn unknown_channel(channel: &str) -> Self {
        Self {
            code: IpcErrorCode::UnknownChannel,
            message: format!("No IPC handler is registered for channel `{channel}`"),
        }
    }

    /// Creates an invalid-payload error.
    pub fn invalid_payload(message: impl Into<String>) -> Self {
        Self {
            code: IpcErrorCode::InvalidPayload,
            message: message.into(),
        }
    }

    /// Creates a duplicate-channel error.
    pub fn duplicate_channel(channel: &str) -> Self {
        Self {
            code: IpcErrorCode::DuplicateChannel,
            message: format!("IPC channel `{channel}` is already registered"),
        }
    }

    /// Creates a handler failure.
    pub fn handler(message: impl Into<String>) -> Self {
        Self {
            code: IpcErrorCode::HandlerFailed,
            message: message.into(),
        }
    }

    /// Creates a response serialization failure.
    pub fn serialization(message: impl Into<String>) -> Self {
        Self {
            code: IpcErrorCode::SerializationFailed,
            message: message.into(),
        }
    }
}

/// TypeScript type metadata supplied by Rust request and response types.
pub trait IpcType {
    /// TypeScript type name used in generated function signatures.
    const TS_NAME: &'static str;
    /// Optional TypeScript declaration emitted once into the generated file.
    const TS_DECLARATION: &'static str;
}

/// Placeholder request type for handlers that take no payload.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct NoRequest;

impl IpcType for NoRequest {
    const TS_NAME: &'static str = "undefined";
    const TS_DECLARATION: &'static str = "";
}

/// Public schema of a registered IPC channel.
#[derive(Debug, Clone)]
pub struct IpcChannelSchema {
    /// Channel name used by `invoke`.
    pub channel: String,
    /// Human-readable channel description.
    pub description: String,
    /// TypeScript request type name.
    pub request_type: &'static str,
    /// TypeScript request declaration.
    pub request_declaration: &'static str,
    /// TypeScript response type name.
    pub response_type: &'static str,
    /// TypeScript response declaration.
    pub response_declaration: &'static str,
}

type Handler = Arc<dyn Fn(Value) -> Result<Value, IpcError> + Send + Sync + 'static>;

#[derive(Clone)]
struct ChannelEntry {
    handler: Handler,
    schema: IpcChannelSchema,
}

/// Registry of typed Rust handlers callable from a WebView.
#[derive(Clone, Default)]
pub struct IpcRouter {
    channels: Arc<RwLock<HashMap<String, ChannelEntry>>>,
}

impl IpcRouter {
    /// Creates an empty IPC router.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a typed request handler.
    ///
    /// The request is decoded from JSON, the handler runs synchronously, and
    /// the response is serialized back to JSON.
    pub fn register<Request, Response, HandlerFn>(
        &self,
        channel: impl Into<String>,
        description: impl Into<String>,
        handler: HandlerFn,
    ) -> Result<(), IpcError>
    where
        Request: IpcType + DeserializeOwned + Send + 'static,
        Response: IpcType + Serialize + 'static,
        HandlerFn: Fn(Request) -> Result<Response, IpcError> + Send + Sync + 'static,
    {
        let channel = channel.into();
        let description = description.into();

        if self
            .channels
            .read()
            .expect("IPC router lock poisoned")
            .contains_key(&channel)
        {
            return Err(IpcError::duplicate_channel(&channel));
        }

        let handler = Arc::new(move |payload: Value| {
            let request: Request = serde_json::from_value(payload)
                .map_err(|error| IpcError::invalid_payload(error.to_string()))?;
            let response = handler(request)?;
            serde_json::to_value(response)
                .map_err(|error| IpcError::serialization(error.to_string()))
        });

        let schema = IpcChannelSchema {
            channel: channel.clone(),
            description,
            request_type: Request::TS_NAME,
            request_declaration: Request::TS_DECLARATION,
            response_type: Response::TS_NAME,
            response_declaration: Response::TS_DECLARATION,
        };

        self.channels
            .write()
            .expect("IPC router lock poisoned")
            .insert(channel, ChannelEntry { handler, schema });

        Ok(())
    }

    /// Dispatches a JSON request body received from `window.ipc.postMessage`.
    pub fn dispatch_json(&self, body: &str) -> IpcReply {
        let request = match serde_json::from_str::<IpcRequest>(body) {
            Ok(request) => request,
            Err(error) => {
                return IpcReply::failure(0, IpcError::invalid_payload(error.to_string()));
            }
        };

        let entry = self
            .channels
            .read()
            .expect("IPC router lock poisoned")
            .get(&request.channel)
            .cloned();

        let Some(entry) = entry else {
            return IpcReply::failure(request.id, IpcError::unknown_channel(&request.channel));
        };

        match (entry.handler)(request.payload) {
            Ok(value) => IpcReply::success(request.id, value),
            Err(error) => IpcReply::failure(request.id, error),
        }
    }

    /// Returns a snapshot of all registered channel schemas.
    pub fn schema(&self) -> Vec<IpcChannelSchema> {
        let mut channels = self
            .channels
            .read()
            .expect("IPC router lock poisoned")
            .values()
            .map(|entry| entry.schema.clone())
            .collect::<Vec<_>>();
        channels.sort_by(|left, right| left.channel.cmp(&right.channel));
        channels
    }
}

/// Returns the JavaScript bridge source injected into every WebView page.
pub(crate) fn bridge_script() -> &'static str {
    BRIDGE_SCRIPT
}

/// Builds JavaScript that resolves a pending IPC promise.
pub(crate) fn reply_script(reply: &IpcReply) -> Result<String, serde_json::Error> {
    let json = serde_json::to_string(reply)?;
    Ok(format!(
        "window.__myrust_ipc__ && window.__myrust_ipc__.__resolve({json});"
    ))
}

/// Builds JavaScript that dispatches a Rust-to-JavaScript event.
pub(crate) fn event_script(channel: &str, payload: &Value) -> Result<String, serde_json::Error> {
    let channel = serde_json::to_string(channel)?;
    let payload = serde_json::to_string(payload)?;
    Ok(format!(
        "window.__myrust_ipc__ && window.__myrust_ipc__.__emit({channel}, {payload});"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct TestResponse {
        ok: bool,
    }

    impl IpcType for TestResponse {
        const TS_NAME: &'static str = "TestResponse";
        const TS_DECLARATION: &'static str = "export interface TestResponse { ok: boolean }";
    }

    #[test]
    fn no_request_accepts_missing_payload() {
        let router = IpcRouter::new();
        router
            .register::<NoRequest, TestResponse, _>("ping", "test", |_| {
                Ok(TestResponse { ok: true })
            })
            .expect("register test handler");

        let reply = router.dispatch_json(r#"{"id":7,"channel":"ping"}"#);
        assert!(reply.ok);
        assert_eq!(reply.id, 7);
        assert_eq!(reply.result.expect("result")["ok"], true);
    }

    #[test]
    fn unknown_channel_returns_structured_error() {
        let router = IpcRouter::new();
        let reply = router.dispatch_json(r#"{"id":9,"channel":"missing"}"#);
        assert!(!reply.ok);
        assert_eq!(reply.error.expect("error").code as i32, 404);
    }
}
