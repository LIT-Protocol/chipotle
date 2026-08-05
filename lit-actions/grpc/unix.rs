use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use hyper_util::rt::TokioIo;
use lit_observability::net::grpc::TracingMiddleware;
use lit_observability::tonic_middleware::MiddlewareLayer;
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::body::BoxBody;
use tonic::transport::{Channel, Endpoint, Server, Uri};
use tracing::warn;

/// Peer-credential (SO_PEERCRED) policy for the control socket.
///
/// The gRPC control socket lets a client run arbitrary code in the runtime
/// with client-cooperative resource accounting (CPL-368). Because the socket
/// lives in a directory shared across the CVM's containers, restrictive file
/// permissions alone are not a hard guarantee, so we additionally verify the
/// connecting peer's UID on every accepted connection and drop anything that
/// isn't us or root.
#[derive(Copy, Clone)]
struct AllowedPeers {
    /// The effective UID of this process. The legitimate client
    /// (lit-api-server / lit-node) runs as the same user in every supported
    /// deployment (all containers share a UID in the Phala CVM; local dev and
    /// the in-process test server share the developer's UID).
    euid: u32,
}

impl AllowedPeers {
    fn current() -> Self {
        // SAFETY: geteuid() is always successful and has no preconditions.
        Self {
            euid: unsafe { libc::geteuid() },
        }
    }

    /// A peer is permitted if it runs as the same user as this process, or as
    /// root. Root is allowed because it can already read the socket and act
    /// as any user; the boundary this enforces is against *unprivileged*
    /// local processes, which are the CPL-368 threat.
    fn permits(&self, peer_uid: u32) -> bool {
        peer_uid == self.euid || peer_uid == 0
    }
}

pub async fn connect_to_socket(socket_path: impl Into<PathBuf>) -> Result<Channel> {
    const IGNORED_URI: &str = "http://[::]:50051";

    // Take a copy before moving into the `service_fn` closure so that the
    // closure can implement `FnMut`.
    let path = socket_path.into();

    // NB: .timeout() doesn't work here for some reason, which is why we use tokio::time::timeout in the client
    Endpoint::from_static(IGNORED_URI)
        .connect_timeout(Duration::from_secs(1))
        .connect_with_connector(tower::service_fn(move |_: Uri| {
            let path = path.clone();
            async move { Ok::<_, std::io::Error>(TokioIo::new(UnixStream::connect(path).await?)) }
        }))
        .await
        .map_err(Into::into)
}

pub async fn start_server<S, P, F>(
    service: S,
    socket_path: P,
    shutdown_signal: Option<F>,
) -> Result<()>
where
    S: tower::Service<
            http::Request<BoxBody>,
            Response = http::Response<BoxBody>,
            Error = std::convert::Infallible,
        > + tonic::server::NamedService
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    P: Into<PathBuf>,
    F: std::future::Future<Output = ()>,
{
    let socket_path = socket_path.into();
    // Probe the path with symlink_metadata (lstat) so a dangling symlink is
    // still detected, and only ever unlink the path *itself* — never a
    // symlink's resolved target. The previous code did read_link() +
    // remove_file(target), which let a symlink planted at the socket path in a
    // world-writable directory (e.g. the default /tmp) trick the daemon into
    // deleting an arbitrary file owned elsewhere (CWE-59). remove_file() unlinks
    // a symlink itself rather than following it, so this still cleans up a stale
    // socket or leftover file at our own path without the file-deletion vector.
    if fs::symlink_metadata(&socket_path).is_ok() {
        fs::remove_file(&socket_path).context("Failed to remove existing socket file")?;
    }

    let uds = UnixListener::bind(socket_path.clone())?;

    // Restrict the socket to owner + group (0o660), dropping the world-writable
    // bit the socket used to carry (0o777). The legitimate client runs as the
    // same user as this process in every supported deployment, so it retains
    // access; unprivileged local processes no longer can. This is the file-mode
    // half of the CPL-368 fix — the SO_PEERCRED check below is the enforced
    // half that does not depend on how the shared volume is mounted.
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o660))?;

    // SO_PEERCRED gate: verify the connecting process's UID before handing the
    // connection to tonic. A connection that fails the check (unauthorized UID,
    // or credentials that cannot be read) is silently dropped so it never
    // reaches `execute_js`. Accept errors are propagated unchanged.
    let allowed = AllowedPeers::current();
    let uds_stream = UnixListenerStream::new(uds).filter_map(move |conn| {
        let stream = match conn {
            Ok(stream) => stream,
            Err(e) => return Some(Err(e)),
        };
        match stream.peer_cred() {
            Ok(cred) if allowed.permits(cred.uid()) => Some(Ok(stream)),
            Ok(cred) => {
                warn!(
                    peer_uid = cred.uid(),
                    peer_pid = cred.pid(),
                    "rejecting control-socket connection from unauthorized peer"
                );
                None
            }
            Err(e) => {
                warn!("rejecting control-socket connection: failed to read peer credentials: {e}");
                None
            }
        }
    });

    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(crate::proto::FILE_DESCRIPTOR_SET)
        .build_v1()?;

    let router = Server::builder()
        .layer(MiddlewareLayer::new(TracingMiddleware))
        .add_service(reflection)
        .add_service(service);

    if let Some(sig) = shutdown_signal {
        router.serve_with_incoming_shutdown(uds_stream, sig).await
    } else {
        router.serve_with_incoming(uds_stream).await
    }?;

    Ok(())
}
