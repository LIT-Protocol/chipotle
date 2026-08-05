//! The `Action` gRPC service front-end: identical protocol surface to the JS
//! runner (`lit-actions/server/server.rs`), served on a different Unix
//! socket. lit-api-server connects with the exact same client code — only
//! the socket path differs.

use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use lit_actions_grpc::{proto::*, unix};
use tokio_stream::{Stream, StreamExt as _};
use tonic::{Request, Response, Status};
use tracing::{debug, error, instrument};

use crate::bridge::OpBridge;
use crate::supervisor::Supervisor;

pub struct GvisorServer {
    supervisor: Arc<Supervisor>,
}

impl GvisorServer {
    pub fn new(supervisor: Supervisor) -> Self {
        Self {
            supervisor: Arc::new(supervisor),
        }
    }

    pub fn into_service(self) -> ActionServer<Self> {
        ActionServer::new(self)
            // Bundles ride in `code`; let lit-api-server enforce size limits.
            .max_decoding_message_size(usize::MAX)
    }
}

#[tonic::async_trait]
impl Action for GvisorServer {
    type ExecuteJsStream =
        Pin<Box<dyn Stream<Item = Result<ExecuteJsResponse, Status>> + Send + 'static>>;

    #[allow(clippy::blocks_in_conditions)]
    #[instrument(skip_all, err)]
    async fn execute_js(
        &self,
        request: Request<tonic::Streaming<ExecuteJsRequest>>,
    ) -> Result<Response<Self::ExecuteJsStream>, Status> {
        let mut stream = request.into_inner();
        // Rendezvous channels, mirroring the JS runner: every op send blocks
        // until the peer is ready, preserving strict one-op-in-flight order.
        let (inbound_tx, inbound_rx) = flume::bounded::<ExecuteJsRequest>(0);
        let (outbound_tx, outbound_rx) = flume::bounded(0);
        let supervisor = self.supervisor.clone();

        // Pump incoming requests into the channel.
        tokio::spawn(async move {
            while let Ok(Some(req)) = stream.try_next().await {
                if inbound_tx
                    .send_async(req)
                    .await
                    .inspect_err(|e| error!("failed to forward request: {e:#}"))
                    .is_err()
                {
                    break;
                }
            }
        });

        // Handle the initial execution request; subsequent messages are op
        // responses consumed by the OpBridge.
        tokio::spawn(async move {
            let first = match inbound_rx.recv_async().await {
                Ok(req) => req,
                Err(e) => {
                    error!("failed to receive request: {e:#}");
                    return;
                }
            };

            #[allow(clippy::single_match)]
            match first.union {
                Some(UnionRequest::Execute(req)) => {
                    // `DebugExecutionRequest` redacts user secrets (js_params,
                    // headers, auth_context) by default (CPL-369), so this is
                    // safe to log unconditionally.
                    debug!("{:?}", DebugExecutionRequest::from(&req));

                    // Empty-code shortcut (parity with the JS runner).
                    if req.code.bytes().all(|b| b.is_ascii_whitespace()) {
                        let _ = outbound_tx
                            .send_async(Ok(ExecutionResult {
                                success: true,
                                ..Default::default()
                            }
                            .into()))
                            .await;
                        return;
                    }

                    let bridge = Arc::new(OpBridge::new(outbound_tx.clone(), inbound_rx));
                    let res = supervisor.run_execution(req, bridge).await;
                    send_execution_result(&outbound_tx, res).await;
                }
                _ => {} // Ignore empty requests
            }
        });

        Ok(Response::new(Box::pin(outbound_rx.into_stream())))
    }
}

/// Convert the supervisor's `Result<()>` into the final stream message.
/// Mirrors the JS runner's `send_execution_result`: `tonic::Status` errors
/// (timeout, cancel) pass through as stream errors; everything else becomes
/// a failed `ExecutionResult`.
async fn send_execution_result(
    outbound_tx: &flume::Sender<tonic::Result<ExecuteJsResponse>>,
    res: Result<()>,
) {
    let response = match res {
        Ok(()) => Ok(ExecutionResult {
            success: true,
            ..Default::default()
        }
        .into()),
        Err(err) => {
            if let Some(status) = err.downcast_ref::<Status>() {
                error!("{status:#}");
                Err(status.clone())
            } else {
                Ok(ExecutionResult {
                    success: false,
                    error: format!("{err:#}"),
                }
                .into())
            }
        }
    };
    let _ = outbound_tx
        .send_async(response)
        .await
        .inspect_err(|e| error!("failed to send execution result: {e:#}"));
}

/// Serve the op-loop on `socket_path`. Same shape as
/// `lit_actions_server::start_server`, reusing the shared UDS bootstrap
/// (permissions, reflection, tracing middleware) from lit-actions-grpc.
pub async fn start_server<P, S>(
    socket_path: P,
    shutdown_signal: Option<S>,
    supervisor: Supervisor,
) -> Result<()>
where
    P: Into<std::path::PathBuf>,
    S: std::future::Future<Output = ()>,
{
    let server = GvisorServer::new(supervisor);
    unix::start_server(server.into_service(), socket_path, shutdown_signal).await
}
