use std::path::PathBuf;
use std::pin::Pin;

use anyhow::Result;
use deno_core::error::CoreError;
use deno_core::futures::TryFutureExt as _;
use deno_lib::util::result::any_and_jserrorbox_downcast_ref;
use deno_runtime::tokio_util::create_and_run_current_thread;
use lit_actions_grpc::{proto::*, unix};
use lit_api_core::context::{HEADER_KEY_X_PRIVACY_MODE, HEADER_KEY_X_REQUEST_ID};
use lit_observability::{
    PRIVACY_MODE_TAG,
    channels::{ChannelMsg, new_traced_bounded_channel},
    logging::{clear_task_request_context, set_request_context},
};
use temp_file::TempFile;
use tokio_stream::{Stream, StreamExt as _};
use tonic::{Request, Response, Status};
use tracing::{Instrument, debug, debug_span, error, instrument};

#[derive(Default, PartialEq)]
pub enum ServerType {
    #[default]
    Production,
    Test,
}

#[derive(Default)]
pub struct Server {
    server_type: ServerType,
}

impl Server {
    fn into_service(self) -> ActionServer<Self> {
        ActionServer::new(self)
            // Let lit_node enforce size limits
            .max_decoding_message_size(usize::MAX)
    }

    fn new_test_server() -> Self {
        Self {
            server_type: ServerType::Test,
        }
    }
}

#[tonic::async_trait]
impl Action for Server {
    type ExecuteJsStream =
        Pin<Box<dyn Stream<Item = Result<ExecuteJsResponse, Status>> + Send + 'static>>;

    #[allow(clippy::blocks_in_conditions)]
    #[instrument(skip_all, err)]
    async fn execute_js(
        &self,
        request: Request<tonic::Streaming<ExecuteJsRequest>>,
    ) -> Result<Response<Self::ExecuteJsStream>, Status> {
        let mut stream = request.into_inner();
        let (inbound_tx, inbound_rx) = new_traced_bounded_channel(0);
        let (outbound_tx, outbound_rx) = flume::bounded(0);
        let is_test_server = self.server_type == ServerType::Test;

        // Put incoming requests into channel
        let send_exec_req_span = debug_span!("send_exec_req");
        tokio::spawn(
            async move {
                while let Ok(Some(req)) = stream.try_next().await {
                    let _ = inbound_tx
                        .send_async(req)
                        .inspect_err(|e| error!("failed to forward request: {e:#}"))
                        .await;
                }
            }
            .instrument(send_exec_req_span),
        );

        // Handle initial execution request, forward ops requests to the runtime
        tokio::spawn(async move {
            let (req, span) = match inbound_rx
                .recv_async()
                .inspect_err(|e| error!("failed to receive request: {e:#}"))
                .await
            {
                Ok(req) => req,
                Err(e) => {
                    error!("failed to receive request: {e:#}");
                    (
                        ChannelMsg::new(ExecuteJsRequest::default()),
                        debug_span!("recv_async"),
                    )
                }
            };
            let req = req.data().to_owned();

            let outbound_tx = outbound_tx.clone();
            let inbound_rx = inbound_rx.clone();

            #[allow(clippy::single_match)]
            match req.union {
                Some(UnionRequest::Execute(req)) => {
                    // we're outside of the spawned thread, so we can't use the request context
                    if req
                        .http_headers
                        .get(&HEADER_KEY_X_PRIVACY_MODE.to_ascii_lowercase())
                        .unwrap_or(&"false".to_string())
                        == "true"
                    {
                        debug!("ExecuteJsRequest: **PRIVACY MODE**");
                    } else {
                        debug!("{:?}", DebugExecutionRequest::from(&req));
                    }

                    std::thread::spawn(move || {
                        // Set request context for log injection; no fallback IDs.
                        let none = &"none".to_string();
                        let privacy_mode = req
                            .http_headers
                            .get(&HEADER_KEY_X_PRIVACY_MODE.to_ascii_lowercase())
                            .unwrap_or(none);
                        let request_id = req
                            .http_headers
                            .get(&HEADER_KEY_X_REQUEST_ID.to_ascii_lowercase())
                            .unwrap_or(none);
                        let request_id = if privacy_mode == "true" {
                            format!("{request_id}_{PRIVACY_MODE_TAG}")
                        } else {
                            request_id.to_string()
                        };

                        if request_id != "none" {
                            set_request_context(Some(request_id), None);
                        }

                        create_and_run_current_thread(
                            async move {
                                let res = crate::runtime::execute_js(
                                    req.code,
                                    req.js_params.and_then(|v| serde_json::from_slice(&v).ok()),
                                    req.auth_context
                                        .and_then(|v| serde_json::from_slice(&v).ok()),
                                    req.http_headers,
                                    req.timeout,
                                    req.memory_limit.map(|limit| limit as usize),
                                    outbound_tx.clone(),
                                    inbound_rx.clone(),
                                    is_test_server,
                                )
                                .await;
                                let _ = outbound_tx
                                    .send_async(match res {
                                        Ok(()) => Ok(ExecutionResult {
                                            success: true,
                                            ..Default::default()
                                        }
                                        .into()),
                                        Err(err) => {
                                            // Return Tonic error as-is, otherwise return ExecutionResult
                                            if let Some(status) = err.downcast_ref::<Status>() {
                                                error!("{status:#}");
                                                Err(status.clone())
                                            } else {
                                                Ok(ExecutionResult {
                                                    success: false,
                                                    error: format_error(&err),
                                                }
                                                .into())
                                            }
                                        }
                                    })
                                    .inspect_err(|e| {
                                        error!("failed to send execution result: {e:#}")
                                    })
                                    .await;
                            }
                            .instrument(span),
                        );
                    });
                }
                _ => {} // Ignore empty requests
            }
        });

        clear_task_request_context();
        Ok(Response::new(Box::pin(outbound_rx.into_stream())))
    }
}

fn format_error(err: &anyhow::Error) -> String {
    if let Some(CoreError::Js(js_err)) = any_and_jserrorbox_downcast_ref::<CoreError>(err) {
        deno_runtime::fmt_errors::format_js_error(js_err)
    } else {
        format!("{err:#}")
    }
}

pub async fn start_server<P, S>(socket_path: P, shutdown_signal: Option<S>) -> Result<()>
where
    P: Into<PathBuf>,
    S: std::future::Future<Output = ()>,
{
    unix::start_server(
        Server::default().into_service(),
        socket_path,
        shutdown_signal,
    )
    .await
}

pub async fn start_test_server<P, S>(socket_path: P, shutdown_signal: Option<S>) -> Result<()>
where
    P: Into<PathBuf>,
    S: std::future::Future<Output = ()>,
{
    unix::start_server(
        Server::new_test_server().into_service(),
        socket_path,
        shutdown_signal,
    )
    .await
}
pub struct TestServer {
    pub socket_file: TempFile,
}

impl TestServer {
    pub fn start() -> Self {
        let socket_file = temp_file::empty();
        let socket_path = socket_file.path().to_path_buf();

        std::thread::spawn(|| {
            create_and_run_current_thread(async move {
                let signal = async {
                    let _ = tokio::signal::ctrl_c().await;
                };
                start_test_server(socket_path, Some(signal))
                    .await
                    .expect("failed to start action server")
            });
        });

        // Wait for startup
        std::thread::sleep(std::time::Duration::from_millis(100));

        Self { socket_file }
    }

    pub fn socket_path(&self) -> PathBuf {
        self.socket_file.path().to_path_buf()
    }
}
