use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};

#[derive(Clone, Debug)]
pub struct GrpcClientPool<C: Clone + std::fmt::Debug> {
    connections: Arc<RwLock<HashMap<String, C>>>,
    create_connection_semaphore: Arc<Semaphore>,
}

impl<C> Default for GrpcClientPool<C>
where
    C: Clone + std::fmt::Debug,
{
    fn default() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            create_connection_semaphore: Arc::new(Semaphore::new(Self::DEFAULT_POOL_SIZE)),
        }
    }
}

impl<C> GrpcClientPool<C>
where
    C: Clone + std::fmt::Debug,
{
    const DEFAULT_POOL_SIZE: usize = 5;

    pub fn new() -> Self {
        let pool_size = Self::DEFAULT_POOL_SIZE;
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            create_connection_semaphore: Arc::new(Semaphore::new(pool_size)),
        }
    }

    pub async fn create_or_get_connection<F, Fut>(&self, addr: &str, create: F) -> anyhow::Result<C>
    where
        Fut: Future<Output = anyhow::Result<C>> + Send,
        F: FnOnce() -> Fut + Send,
    {
        let span = tracing::debug_span!(
            "grpc_pool::create_or_get_connection",
            addr,
            reused = tracing::field::Empty
        );
        let _guard = span.enter();
        loop {
            tracing::debug!("Connecting to address: {}", addr);
            let conn = self.get_connection(addr).await;
            match conn {
                Some(client) => {
                    tracing::Span::current().record("reused", true);
                    tracing::trace!("Reusing existing grpc client connection at {}", addr);
                    return Ok(client);
                }
                None => {
                    match self.create_connection_semaphore.try_acquire() {
                        Ok(_permit) => {
                            tracing::Span::current().record("reused", false);
                            tracing::trace!("Creating a new grpc client connection at {}", addr);
                            let c = match create().await {
                                Ok(c) => c,
                                Err(e) => {
                                    tracing::error!("Error creating client: {}", e);
                                    return Err(e);
                                }
                            };
                            return Ok(self.add_connection(addr, c).await);
                        }
                        Err(tokio::sync::TryAcquireError::Closed) => {
                            return Err(anyhow::anyhow!(
                                "Semaphore for creating grpc connections is closed",
                            ));
                        }
                        Err(tokio::sync::TryAcquireError::NoPermits) => {
                            tracing::debug!("grpc_pool: no permits, waiting 50ms");
                            // Wait a bit before retrying to avoid busy waiting
                            // There's new connections being created so we don't need to try
                            // to create one, just wait for it one to be done and reuse it
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        }
                    }
                }
            }
        }
    }

    pub async fn get_connection(&self, addr: &str) -> Option<C> {
        self.connections.read().await.get(addr).cloned()
    }

    pub async fn add_connection(&self, addr: &str, client: C) -> C {
        self.connections
            .write()
            .await
            .entry(addr.to_string())
            .or_insert(client.clone());
        client
    }

    pub async fn get_addresses(&self) -> Vec<String> {
        self.connections.read().await.keys().cloned().collect()
    }

    pub async fn remove_connection(&self, addr: &str) {
        self.connections.write().await.remove(addr);
    }

    pub async fn remove_connections(&self, addresses: &[String]) {
        let mut connections = self.connections.write().await;
        for addr in addresses {
            connections.remove(addr);
        }
    }
}
