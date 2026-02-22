//! This module implements job management for async Lit Actions.
//! It uses the [Apalis library](https://github.com/geofmureithi/apalis)
//! and SQLite for job scheduling and execution.

use std::future::Future;
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr as _;
use std::time::Duration;

use crate::error::Error;

use anyhow::Result;
use apalis::{layers::tracing::TraceLayer, prelude::*};
use apalis_sql::{
    context::SqlContext,
    sqlite::{SqlitePool, SqliteStorage},
    sqlx::{self, ConnectOptions as _, sqlite::SqliteConnectOptions},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::client::Client;
use super::client::models::{DenoExecutionEnv, ExecutionOptions, ExecutionState};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobId(TaskId);

impl From<TaskId> for JobId {
    fn from(task_id: TaskId) -> Self {
        Self(task_id)
    }
}

impl Deref for JobId {
    type Target = TaskId;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Scheduled,
    Running,
    Succeeded,
    Failed,
    Killed,
}

impl From<State> for JobStatus {
    fn from(state: State) -> Self {
        match state {
            State::Pending => Self::Pending,
            State::Scheduled => Self::Scheduled,
            State::Running => Self::Running,
            State::Done => Self::Succeeded,
            State::Failed => Self::Failed,
            State::Killed => Self::Killed,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ActionJob {
    client: Client,
    opts: ExecutionOptions,
}

impl ActionJob {
    pub fn new(client: Client, opts: impl Into<ExecutionOptions>) -> Self {
        Self {
            client,
            opts: opts.into(),
        }
    }

    pub async fn run(&mut self) -> Result<ExecutionState, Error> {
        self.client.execute_js(self.opts.clone()).await
    }

    pub async fn run_with_env(&mut self, env: DenoExecutionEnv) -> Result<ExecutionState, Error> {
        self.client.js_env = env;
        self.run().await
    }

    pub fn state(&self) -> &ExecutionState {
        &self.client.state
    }

    pub fn authorized_address(&self) -> Option<String> {
        // self.client.authorized_address()
        Some("127.0.0.1:8080".to_string())
    }
}

pub type ActionStore = JobStore<ActionJob, Result<ExecutionState, Error>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job<T, R> {
    pub id: JobId,
    pub status: JobStatus,
    pub result: Option<R>,
    pub completed_at: Option<i64>,
    pub payload: T,
}

impl<T, R> Deref for Job<T, R> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.payload
    }
}

#[derive(Debug, Clone)]
pub struct JobStore<T, R> {
    inner: SqliteStorage<T>,
    result: PhantomData<R>,
}

impl<T, R> From<SqliteStorage<T>> for JobStore<T, R> {
    fn from(storage: SqliteStorage<T>) -> Self {
        Self {
            inner: storage,
            result: PhantomData,
        }
    }
}

impl<T, R> JobStore<T, R>
where
    T: 'static + Send + Sync + Unpin + Serialize + DeserializeOwned + std::fmt::Debug,
    R: DeserializeOwned,
{
    pub async fn new(db_path: &str) -> Result<Self> {
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::from_str(db_path)?
                .create_if_missing(true)
                // Same as log::LevelFilter::Trace without depending on the log crate
                .log_statements("trace".parse().expect("valid log level")),
        )
        .await?;

        // Tweak SQLite settings and run migrations
        SqliteStorage::setup(&pool).await?;

        let config = apalis_sql::Config::default()
            .set_namespace(std::any::type_name::<T>())
            // Reduce polling interval from default 100ms to something more reasonable
            .set_poll_interval(Duration::from_millis(1000));

        Ok(SqliteStorage::<T>::new_with_config(pool, config).into())
    }

    pub async fn new_in_memory() -> Result<Self> {
        Self::new(":memory:").await
    }

    pub async fn submit_job(&mut self, job: T) -> Result<JobId> {
        let req = {
            let mut ctx = SqlContext::new();
            ctx.set_max_attempts(1); // no retries
            Request::new_with_ctx(job, ctx)
        };
        let res = self.inner.push_request(req).await?;
        Ok(res.task_id.into())
    }

    pub async fn get_job(&mut self, job_id: &JobId) -> Result<Option<Job<T, R>>> {
        match self.inner.fetch_by_id(job_id).await? {
            Some(res) => {
                let (payload, parts) = res.take_parts();
                let ctx = parts.context;
                Ok(Some(Job {
                    id: parts.task_id.into(),
                    status: ctx.status().clone().into(),
                    result: ctx
                        .last_error()
                        .as_ref()
                        .map(|e| serde_json::from_str(&e.replace("FailedError: ", "")))
                        .transpose()?,
                    completed_at: *ctx.done_at(),
                    payload,
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn delete_all_jobs(&self) -> Result<u64> {
        let query = "DELETE FROM Jobs";
        let res = sqlx::query(query).execute(self.inner.pool()).await?;
        Ok(res.rows_affected())
    }

    pub async fn delete_completed_jobs(&self, max_age: Duration) -> Result<u64> {
        let query =
            "DELETE FROM Jobs WHERE done_at IS NOT NULL AND done_at < strftime('%s', 'now') - ?1";
        let res = sqlx::query(query)
            .bind(max_age.as_secs() as i64)
            .execute(self.inner.pool())
            .await?;
        Ok(res.rows_affected())
    }

    pub fn into_inner(self) -> SqliteStorage<T> {
        self.inner
    }
}
pub struct ActionWorker {
    monitor: Monitor,
}

impl ActionWorker {
    pub fn new(store: ActionStore, env: DenoExecutionEnv) -> Self {
        async fn run_job(
            mut job: ActionJob,
            env: Data<DenoExecutionEnv>,
        ) -> Result<ExecutionState, Error> {
            job.run_with_env(env.deref().clone()).await
        }

        let monitor = Monitor::new().register({
            WorkerBuilder::new("action-worker")
                .layer(TraceLayer::new())
                .data(env)
                .backend(store.into_inner())
                .build_fn(run_job)
        });

        Self { monitor }
    }

    pub async fn start(self) -> Result<()> {
        self.monitor.run().await.map_err(Into::into)
    }

    pub async fn start_with_shutdown<S>(self, shutdown_signal: S) -> Result<()>
    where
        S: Future<Output = std::io::Result<()>> + Send,
    {
        self.monitor
            .run_with_signal(shutdown_signal)
            .await
            .map_err(Into::into)
    }
}
