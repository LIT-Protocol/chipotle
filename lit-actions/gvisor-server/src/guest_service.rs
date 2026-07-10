//! The per-execution `GuestOps` gRPC service served on the sandbox's op
//! socket. Every handler is a one-line forward through the `OpBridge` onto
//! the op-loop stream, so op semantics live entirely in lit-api-server.

use std::sync::Arc;

use lit_actions_grpc::proto::*;
use tonic::{Request, Response, Status};

use crate::bridge::OpBridge;
use crate::proto::{GetJobRequest, GuestOps, Job};

pub(crate) struct GuestOpsService {
    pub bridge: Arc<OpBridge>,
    pub job: Job,
}

/// Op failures (including `ReportError` text from lit-api-server) flow back
/// to the guest verbatim so the `lit` CLI can surface them to user code.
fn op_err(e: anyhow::Error) -> Status {
    Status::internal(format!("{e:#}"))
}

macro_rules! forward {
    ($self:ident, $req:ident, $op:ident) => {
        $self
            .bridge
            .$op($req.into_inner())
            .await
            .map(Response::new)
            .map_err(op_err)
    };
}

#[tonic::async_trait]
impl GuestOps for GuestOpsService {
    async fn get_job(&self, _req: Request<GetJobRequest>) -> Result<Response<Job>, Status> {
        Ok(Response::new(self.job.clone()))
    }

    async fn print(&self, req: Request<PrintRequest>) -> Result<Response<PrintResponse>, Status> {
        forward!(self, req, print)
    }

    async fn set_response(
        &self,
        req: Request<SetResponseRequest>,
    ) -> Result<Response<SetResponseResponse>, Status> {
        forward!(self, req, set_response)
    }

    async fn increment_fetch_count(
        &self,
        req: Request<IncrementFetchCountRequest>,
    ) -> Result<Response<IncrementFetchCountResponse>, Status> {
        forward!(self, req, increment_fetch_count)
    }

    async fn aes_encrypt(
        &self,
        req: Request<AesEncryptRequest>,
    ) -> Result<Response<AesEncryptResponse>, Status> {
        forward!(self, req, aes_encrypt)
    }

    async fn aes_decrypt(
        &self,
        req: Request<AesDecryptRequest>,
    ) -> Result<Response<AesDecryptResponse>, Status> {
        forward!(self, req, aes_decrypt)
    }

    async fn get_private_key(
        &self,
        req: Request<GetPrivateKeyRequest>,
    ) -> Result<Response<GetPrivateKeyResponse>, Status> {
        forward!(self, req, get_private_key)
    }

    async fn get_lit_action_private_key(
        &self,
        req: Request<GetLitActionPrivateKeyRequest>,
    ) -> Result<Response<GetLitActionPrivateKeyResponse>, Status> {
        forward!(self, req, get_lit_action_private_key)
    }

    async fn get_lit_action_public_key(
        &self,
        req: Request<GetLitActionPublicKeyRequest>,
    ) -> Result<Response<GetLitActionPublicKeyResponse>, Status> {
        forward!(self, req, get_lit_action_public_key)
    }

    async fn get_lit_action_wallet_address(
        &self,
        req: Request<GetLitActionWalletAddressRequest>,
    ) -> Result<Response<GetLitActionWalletAddressResponse>, Status> {
        forward!(self, req, get_lit_action_wallet_address)
    }
}
