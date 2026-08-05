#![allow(clippy::unwrap_used, clippy::ignored_unit_patterns)]
tonic::include_proto!("com.litprotocol.actions");

pub const FILE_DESCRIPTOR_SET: &[u8] =
    tonic::include_file_descriptor_set!("lit_actions_descriptor");

// The following makes the generated code more ergonomic to use

pub use action_client::ActionClient;
pub use action_server::{Action, ActionServer};

pub use execute_js_request::ErrorResponse;
pub use execute_js_request::{ExecutionRequest, Union as UnionRequest};
pub use execute_js_response::{ExecutionResult, Union as UnionResponse};

impl From<&str> for ExecutionRequest {
    fn from(code: &str) -> Self {
        Self {
            code: code.to_string(),
            ..Default::default()
        }
    }
}

impl From<String> for ExecutionRequest {
    fn from(code: String) -> Self {
        Self {
            code,
            ..Default::default()
        }
    }
}

impl From<ExecutionRequest> for ExecuteJsRequest {
    fn from(req: ExecutionRequest) -> Self {
        Self {
            union: Some(UnionRequest::Execute(req)),
        }
    }
}

impl From<ExecutionResult> for ExecuteJsResponse {
    fn from(res: ExecutionResult) -> Self {
        Self {
            union: Some(UnionResponse::Result(res)),
        }
    }
}

impl From<ErrorResponse> for ExecuteJsRequest {
    fn from(req: ErrorResponse) -> Self {
        Self {
            union: Some(UnionRequest::ReportError(req)),
        }
    }
}

// A wrapper for ExecutionRequest with a custom Debug impl
pub struct DebugExecutionRequest<'a>(&'a ExecutionRequest);

impl<'a> From<&'a ExecutionRequest> for DebugExecutionRequest<'a> {
    fn from(req: &'a ExecutionRequest) -> Self {
        Self(req)
    }
}

impl std::fmt::Debug for DebugExecutionRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const MAX_CODE_LEN: usize = 500;

        let Self(req) = self;

        let truncated_code = if req.code.len() > MAX_CODE_LEN {
            &format!(
                "{}... (truncated, {} bytes total)",
                &req.code[..MAX_CODE_LEN],
                req.code.len()
            )
        } else {
            req.code.as_str()
        };

        // `js_params`, `auth_context`, and `http_headers` carry user-supplied
        // secrets. Redact them by default so they never reach logs / the GCP
        // export; operators opt in for local debugging via
        // LIT_LOG_SENSITIVE_DATA (CPL-369). Redaction here does NOT depend on
        // any client-controlled signal.
        const REDACTED: &str = "<redacted>";
        let log_sensitive = lit_observability::sensitive_logging_enabled();

        let mut s = f.debug_struct("ExecutionRequest");
        s.field("code", &truncated_code);
        if log_sensitive {
            s.field("js_params", &req.js_params)
                .field("auth_context", &req.auth_context);
        } else {
            s.field("js_params", &REDACTED)
                .field("auth_context", &REDACTED);
        }
        s.field("timeout", &req.timeout)
            .field("memory_limit", &req.memory_limit);
        if log_sensitive {
            s.field("http_headers", &req.http_headers);
        } else {
            s.field("http_headers", &REDACTED);
        }
        s.field("ipfs_id", &req.ipfs_id)
            .field(
                "startup_script",
                &req.startup_script.as_ref().map(String::len),
            )
            .finish()
    }
}

// Declare op request/response types
// For example, decl_op!(Print) will declare PrintRequest and PrintResponse
// as well as conversions to and from ExecuteJsRequest and ExecuteJsResponse
macro_rules! decl_op {
    ($prefix:ident) => {
        concat_idents::concat_idents!(typ = $prefix, Response {
            pub use execute_js_request::typ;
            impl From<typ> for ExecuteJsRequest {
                fn from(resp: typ) -> Self {
                    Self {
                        union: Some(UnionRequest::$prefix(resp)),
                    }
                }
            }
        });
        concat_idents::concat_idents!(typ = $prefix, Request {
            pub use execute_js_response::typ;
            impl From<typ> for ExecuteJsResponse {
                fn from(req: typ) -> Self {
                    Self {
                        union: Some(UnionResponse::$prefix(req)),
                    }
                }
            }
        });
    };
}

decl_op!(AesDecrypt);
decl_op!(AesEncrypt);
decl_op!(GetPrivateKey);
decl_op!(GetLitActionPrivateKey);
decl_op!(GetLitActionPublicKey);
decl_op!(GetLitActionWalletAddress);
decl_op!(IncrementFetchCount);
decl_op!(Print);
decl_op!(SetResponse);
decl_op!(UpdateResourceUsage);

#[cfg(test)]
mod tests {
    use super::*;

    /// By default (no `LIT_LOG_SENSITIVE_DATA` opt-out), the Debug rendering used
    /// for logging must never expose user-supplied secrets in `js_params`,
    /// `auth_context`, or `http_headers` (CPL-369). This asserts the redacted
    /// path; the test process does not set the opt-out env var.
    #[test]
    fn debug_redacts_user_secrets_by_default() {
        assert!(
            !lit_observability::sensitive_logging_enabled(),
            "test env must not set LIT_LOG_SENSITIVE_DATA"
        );

        let mut req = ExecutionRequest {
            code: "console.log('hi')".to_string(),
            js_params: Some(b"SECRET_PARAM_VALUE".to_vec()),
            auth_context: Some(b"SECRET_AUTH_SIG".to_vec()),
            ..Default::default()
        };
        req.http_headers.insert(
            "authorization".to_string(),
            "SECRET_BEARER_TOKEN".to_string(),
        );

        let rendered = format!("{:?}", DebugExecutionRequest::from(&req));

        // Secrets must not leak.
        assert!(
            !rendered.contains("SECRET_PARAM_VALUE"),
            "js_params leaked: {rendered}"
        );
        assert!(
            !rendered.contains("SECRET_AUTH_SIG"),
            "auth_context leaked: {rendered}"
        );
        assert!(
            !rendered.contains("SECRET_BEARER_TOKEN"),
            "header value leaked: {rendered}"
        );
        assert!(
            !rendered.contains("authorization"),
            "header name leaked: {rendered}"
        );

        // Sensitive fields render as the redaction placeholder; non-sensitive
        // fields (code) remain visible for debugging.
        assert!(
            rendered.contains("<redacted>"),
            "expected redaction marker: {rendered}"
        );
        assert!(
            rendered.contains("console.log"),
            "code should stay visible: {rendered}"
        );
    }
}
