use std::{collections::HashMap, future::Future};

use crate::{opentelemetry::global, tracing::propagation::HashMapMetadataMap};
use flume::{
    Receiver, RecvError, SendError, Sender,
    r#async::{RecvFut, SendFut},
};
use tracing::{Instrument, debug_span, instrument};
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Create a new bounded channel with tracing support.
pub fn new_traced_bounded_channel<T>(cap: usize) -> (TracedSender<T>, TracedReceiver<T>)
where
    T: std::fmt::Debug,
{
    let (tx, rx) = flume::bounded::<ChannelMsg<T>>(cap);
    (TracedSender { inner: tx }, TracedReceiver { inner: rx })
}

/// Create a new unbounded channel with tracing support.
pub fn new_traced_unbounded_channel<T>() -> (TracedSender<T>, TracedReceiver<T>)
where
    T: std::fmt::Debug,
{
    let (tx, rx) = flume::unbounded::<ChannelMsg<T>>();
    (TracedSender { inner: tx }, TracedReceiver { inner: rx })
}

/// A `flume::Sender` that also injects tracing context into the metadata of the message.
#[derive(Debug)]
pub struct TracedSender<T>
where
    T: std::fmt::Debug,
{
    inner: Sender<ChannelMsg<T>>,
}

impl<T> TracedSender<T>
where
    T: std::fmt::Debug,
{
    /// Send a value to the channel and inject tracing context into the metadata of the message.
    #[instrument(level = "debug", name = "traced_send", skip_all)]
    pub fn send(&self, data: T) -> Result<(), SendError<ChannelMsg<T>>> {
        // Inject tracing context into metadata.
        let mut metadata = HashMap::new();
        let cx = tracing::Span::current().context();

        global::get_text_map_propagator(|propagator| {
            propagator.inject_context(&cx, &mut HashMapMetadataMap(&mut metadata))
        });

        self.inner.send(ChannelMsg { metadata, data })
    }

    /// Send a value to the channel and inject tracing context into the metadata of the message.
    #[instrument(level = "debug", name = "traced_send_async", skip_all)]
    pub fn send_async(&self, data: T) -> SendFut<'_, ChannelMsg<T>> {
        // Inject tracing context into metadata.
        let mut metadata = HashMap::new();
        let cx = tracing::Span::current().context();

        global::get_text_map_propagator(|propagator| {
            propagator.inject_context(&cx, &mut HashMapMetadataMap(&mut metadata))
        });

        self.inner.send_async(ChannelMsg { metadata, data })
    }
}

impl<T> Clone for TracedSender<T>
where
    T: std::fmt::Debug,
{
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

/// A `flume::Receiver` that also extracts tracing context from the metadata of the message.
#[derive(Debug)]
pub struct TracedReceiver<T>
where
    T: std::fmt::Debug,
{
    inner: Receiver<ChannelMsg<T>>,
}

impl<T> TracedReceiver<T>
where
    T: std::fmt::Debug,
{
    /// Receive a value from the channel and return a span. If you wish to correlate all subsequent
    /// spans to be a child of this span, you MUST use the returned span to instrument
    /// all subsequent functions.
    ///
    /// The intention is to allow for establishing the following hierarchy of spans:
    /// - sender span
    ///   - recv span
    ///     - consumer span
    ///       - <work done in consumer>
    pub fn recv(&self) -> Result<(ChannelMsg<T>, tracing::Span), RecvError> {
        let recv_span = debug_span!("traced_recv");

        let mut msg = recv_span.in_scope(|| self.inner.recv())?;

        // Extract the propagated tracing context from the incoming request headers.
        let parent_cx = global::get_text_map_propagator(|propagator| {
            propagator.extract(&HashMapMetadataMap(&mut msg.metadata))
        });

        // Set parent of recv_span to be the parent_cx that is propagated from the sender.
        recv_span.set_parent(parent_cx);

        // Create a new span for the work done in the receiver.
        let consumer_span = debug_span!("traced_consumer");

        // Set the parent of the consumer_span to be the recv_span.
        consumer_span.set_parent(recv_span.context());

        Ok((msg, consumer_span))
    }

    /// Receive a value from the channel and return a span. If you wish to correlate all subsequent
    /// spans to be a child of this span, you MUST use the returned span to instrument
    /// all subsequent functions.
    ///
    /// The intention is to allow for establishing the following hierarchy of spans:
    /// - sender span
    ///   - recv span
    ///     - consumer span
    ///       - <work done in consumer>
    pub async fn recv_async(
        &self,
    ) -> <RecvFut<'_, (ChannelMsg<T>, tracing::Span)> as Future>::Output {
        let recv_span = debug_span!("traced_recv_async");

        let mut msg = self.inner.recv_async().instrument(recv_span.clone()).await?;

        // Extract the propagated tracing context from the incoming request headers.
        let parent_cx = global::get_text_map_propagator(|propagator| {
            propagator.extract(&HashMapMetadataMap(&mut msg.metadata))
        });

        // Set parent of recv_span to be the parent_cx that is propagated from the sender.
        recv_span.set_parent(parent_cx);

        // Create a new span for the work done in the receiver.
        let consumer_span = debug_span!("traced_consumer");

        // Set the parent of the consumer_span to be the recv_span.
        consumer_span.set_parent(recv_span.context());

        Ok((msg, consumer_span))
    }
}

impl<T> Clone for TracedReceiver<T>
where
    T: std::fmt::Debug,
{
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

/// A message that is sent through the channel with metadata to accommodate for tracing context and other metadata.
pub struct ChannelMsg<T>
where
    T: std::fmt::Debug,
{
    metadata: HashMap<String, String>,
    data: T,
}

impl<T> ChannelMsg<T>
where
    T: std::fmt::Debug,
{
    /// Create a new channel message.
    pub fn new(data: T) -> Self {
        Self { metadata: HashMap::new(), data }
    }

    /// Get the metadata of the message.
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    /// Get the data of the message.
    pub fn data(&self) -> &T {
        &self.data
    }
}
