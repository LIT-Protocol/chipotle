//! Serialized proxy between sandbox-side op producers and the ExecuteJs
//! op-loop stream to lit-api-server.
//!
//! The op-loop protocol allows exactly ONE op in flight: the runner sends an
//! `ExecuteJsResponse` (op request) and lit-api-server replies with exactly
//! one `ExecuteJsRequest` (the matching op response, or `ReportError`). The
//! JS runner gets this ordering for free from V8's single thread; here the
//! producers are concurrent (guest RPCs, supervisor usage ticks, stdout/
//! stderr forwarding), so a tokio `Mutex` serializes each send/recv pair.

use anyhow::{Context as _, Result, bail};
use lit_actions_grpc::proto::*;
use tokio::sync::Mutex;

pub struct OpBridge {
    outbound_tx: flume::Sender<tonic::Result<ExecuteJsResponse>>,
    inbound_rx: flume::Receiver<ExecuteJsRequest>,
    lock: Mutex<()>,
}

/// Declare a typed op helper: sends the op request upstream and unwraps the
/// matching response variant (or surfaces `ReportError` as an `Err`).
macro_rules! op {
    ($fn_name:ident, $req:ty, $variant:ident, $resp:ty) => {
        pub async fn $fn_name(&self, req: $req) -> Result<$resp> {
            match self.round_trip(req.into()).await? {
                UnionRequest::$variant(resp) => Ok(resp),
                other => bail!(
                    "{}: unexpected op response: {other:?}",
                    stringify!($fn_name)
                ),
            }
        }
    };
}

impl OpBridge {
    pub fn new(
        outbound_tx: flume::Sender<tonic::Result<ExecuteJsResponse>>,
        inbound_rx: flume::Receiver<ExecuteJsRequest>,
    ) -> Self {
        Self {
            outbound_tx,
            inbound_rx,
            lock: Mutex::new(()),
        }
    }

    /// Send one op request and await its response, holding the bridge lock so
    /// concurrent producers can never interleave a send between another
    /// producer's send and recv.
    async fn round_trip(&self, op: ExecuteJsResponse) -> Result<UnionRequest> {
        let _guard = self.lock.lock().await;
        self.outbound_tx
            .send_async(Ok(op))
            .await
            .context("op-loop stream closed while sending op")?;
        let resp = self
            .inbound_rx
            .recv_async()
            .await
            .context("op-loop stream closed while awaiting op response")?;
        match resp.union {
            Some(UnionRequest::ReportError(ErrorResponse { error })) => bail!(error),
            Some(union) => Ok(union),
            None => bail!("empty op response from lit-api-server"),
        }
    }

    op!(print, PrintRequest, Print, PrintResponse);
    op!(
        set_response,
        SetResponseRequest,
        SetResponse,
        SetResponseResponse
    );
    op!(
        increment_fetch_count,
        IncrementFetchCountRequest,
        IncrementFetchCount,
        IncrementFetchCountResponse
    );
    op!(
        aes_encrypt,
        AesEncryptRequest,
        AesEncrypt,
        AesEncryptResponse
    );
    op!(
        aes_decrypt,
        AesDecryptRequest,
        AesDecrypt,
        AesDecryptResponse
    );
    op!(
        get_private_key,
        GetPrivateKeyRequest,
        GetPrivateKey,
        GetPrivateKeyResponse
    );
    op!(
        get_lit_action_private_key,
        GetLitActionPrivateKeyRequest,
        GetLitActionPrivateKey,
        GetLitActionPrivateKeyResponse
    );
    op!(
        get_lit_action_public_key,
        GetLitActionPublicKeyRequest,
        GetLitActionPublicKey,
        GetLitActionPublicKeyResponse
    );
    op!(
        get_lit_action_wallet_address,
        GetLitActionWalletAddressRequest,
        GetLitActionWalletAddress,
        GetLitActionWalletAddressResponse
    );
    op!(
        update_resource_usage,
        UpdateResourceUsageRequest,
        UpdateResourceUsage,
        UpdateResourceUsageResponse
    );
}
