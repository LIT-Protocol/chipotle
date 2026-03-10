//! Context-aware OpenTelemetry log layer.
//! Injects request_id/correlation_id into OTLP logs.
//! Resolution order: span extensions, then task-local context keyed by tokio task ID.

use std::any::TypeId;
use std::borrow::Cow;
use std::marker::PhantomData;
use std::sync::LazyLock;

use dashmap::DashMap;

use opentelemetry::Key;
use opentelemetry::logs::{AnyValue, LogRecord as _, Logger, LoggerProvider as _, Severity};
use opentelemetry::trace::TraceContextExt;
use opentelemetry_sdk::logs::LoggerProvider;
use tracing::span::{Attributes, Id, Record};
use tracing::{Dispatch, Event, Span, Subscriber};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

const INSTRUMENTATION_LIBRARY_NAME: &str = "lit-observability";

// Task-local fallback keyed by tokio task ID; cleared at request boundaries.
// Uses DashMap for sharded locking to reduce contention under high concurrency.
static TASK_CONTEXTS: LazyLock<DashMap<tokio::task::Id, RequestContext>> =
    LazyLock::new(DashMap::new);

/// Request context propagated to all log events within a span hierarchy.
#[derive(Clone, Debug, Default)]
pub struct RequestContext {
    pub request_id: Option<String>,
    pub correlation_id: Option<String>,
}

impl RequestContext {
    pub fn new(request_id: Option<String>, correlation_id: Option<String>) -> Self {
        Self { request_id, correlation_id }
    }

    pub fn has_context(&self) -> bool {
        self.request_id.is_some() || self.correlation_id.is_some()
    }
}

/// Helper for setting request context via `downcast_raw`.
pub(crate) struct WithRequestContext(fn(dispatch: &Dispatch, id: &Id, ctx: &RequestContext));

impl WithRequestContext {
    pub(crate) fn set_context(&self, dispatch: &Dispatch, id: &Id, ctx: &RequestContext) {
        (self.0)(dispatch, id, ctx)
    }
}

/// Tracing layer that converts events to OpenTelemetry LogRecords with request context injection.
pub struct ContextAwareOtelLogLayer<S> {
    logger: opentelemetry_sdk::logs::Logger,
    with_context: WithRequestContext,
    get_context: GetRequestContext,
    _subscriber: PhantomData<fn(S)>,
}

impl<S> ContextAwareOtelLogLayer<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    pub fn new(provider: &LoggerProvider) -> Self {
        Self {
            logger: provider
                .logger_builder(INSTRUMENTATION_LIBRARY_NAME)
                .with_version(Cow::Borrowed(env!("CARGO_PKG_VERSION")))
                .build(),
            with_context: WithRequestContext(Self::set_context_impl),
            get_context: GetRequestContext(Self::get_context_impl),
            _subscriber: PhantomData,
        }
    }

    fn set_context_impl(dispatch: &Dispatch, id: &Id, ctx: &RequestContext) {
        if let Some(subscriber) = dispatch.downcast_ref::<S>()
            && let Some(span) = subscriber.span(id)
        {
            span.extensions_mut().insert(ctx.clone());
        }
    }

    fn get_context_impl(dispatch: &Dispatch, id: &Id) -> Option<RequestContext> {
        let subscriber = dispatch.downcast_ref::<S>()?;
        let span = subscriber.span(id)?;

        // Walk the span hierarchy (scope() includes current span first, then ancestors)
        // This allows child spans to find context set on parent spans
        for ancestor in span.scope() {
            if let Some(ctx) = ancestor.extensions().get::<RequestContext>()
                && ctx.has_context()
            {
                return Some(ctx.clone());
            }
        }
        None
    }

    fn resolve_request_context(
        &self, ctx: &Context<'_, S>, event: &Event<'_>,
    ) -> Option<RequestContext> {
        // Priority 1: Walk span ancestry to find request context in extensions.
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope {
                if let Some(request_ctx) = span.extensions().get::<RequestContext>()
                    && request_ctx.has_context()
                {
                    return Some(request_ctx.clone());
                }
            }
        }

        // Priority 2: Fall back to task-local context (async-safe).
        get_task_request_context()
    }
}

impl<S> Layer<S> for ContextAwareOtelLogLayer<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, _attrs: &Attributes<'_>, _id: &Id, _ctx: Context<'_, S>) {}

    fn on_record(&self, _span: &Id, _values: &Record<'_>, _ctx: Context<'_, S>) {}

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut log_record = self.logger.create_log_record();

        // Inject trace context from current OTel span for log-trace correlation.
        // Note: Uses Context::current(), so logs with explicit `parent:` spans may correlate incorrectly.
        let otel_ctx = opentelemetry::Context::current();
        if otel_ctx.has_active_span() {
            let otel_span = otel_ctx.span();
            let span_context = otel_span.span_context();
            if span_context.is_valid() {
                log_record.trace_context = Some(span_context.into());
            }
        }

        let severity = match *event.metadata().level() {
            tracing::Level::TRACE => Severity::Trace,
            tracing::Level::DEBUG => Severity::Debug,
            tracing::Level::INFO => Severity::Info,
            tracing::Level::WARN => Severity::Warn,
            tracing::Level::ERROR => Severity::Error,
        };
        log_record.set_severity_number(severity);
        log_record.set_severity_text(event.metadata().level().to_string().into());
        log_record.set_target(event.metadata().target().to_string());
        log_record.set_event_name(event.metadata().name());

        let mut visitor = EventVisitor::new(&mut log_record);
        event.record(&mut visitor);
        let context_fields = visitor.into_recorded_context_fields();

        if (!context_fields.has_request_id || !context_fields.has_correlation_id)
            && let Some(request_ctx) = self.resolve_request_context(&ctx, event)
        {
            // Only add attributes not already present from event fields
            if !context_fields.has_request_id
                && let Some(ref request_id) = request_ctx.request_id
            {
                log_record
                    .add_attribute(Key::new("request_id"), AnyValue::from(request_id.clone()));
            }
            if !context_fields.has_correlation_id
                && let Some(ref correlation_id) = request_ctx.correlation_id
            {
                log_record.add_attribute(
                    Key::new("correlation_id"),
                    AnyValue::from(correlation_id.clone()),
                );
            }
        }

        self.logger.emit(log_record);
    }

    /// # Safety
    ///
    /// This implements the `downcast_raw` method required by `tracing_subscriber::Layer`
    /// to enable type-safe access to this layer's helper types via `Dispatch::downcast_ref`.
    ///
    /// Safety invariants upheld:
    /// - All returned pointers point to fields owned by `self` (`with_context`, `get_context`)
    /// - Pointers remain valid for the `&self` lifetime (layer lifetime matches subscriber)
    /// - The tracing-subscriber machinery guarantees pointers aren't stored beyond the call
    /// - This follows the standard pattern from `tracing-subscriber` and `tracing-opentelemetry`
    unsafe fn downcast_raw(&self, id: TypeId) -> Option<*const ()> {
        match id {
            id if id == TypeId::of::<Self>() => Some(self as *const _ as *const ()),
            id if id == TypeId::of::<WithRequestContext>() => {
                Some(&self.with_context as *const _ as *const ())
            }
            id if id == TypeId::of::<GetRequestContext>() => {
                Some(&self.get_context as *const _ as *const ())
            }
            _ => None,
        }
    }
}

#[derive(Default)]
struct RecordedContextFields {
    has_request_id: bool,
    has_correlation_id: bool,
}

/// Extracts tracing event fields into a LogRecord, preserving native types.
struct EventVisitor<'a, LR: opentelemetry::logs::LogRecord> {
    log_record: &'a mut LR,
    context_fields: RecordedContextFields,
}

impl<'a, LR: opentelemetry::logs::LogRecord> EventVisitor<'a, LR> {
    fn new(log_record: &'a mut LR) -> Self {
        Self { log_record, context_fields: RecordedContextFields::default() }
    }

    fn into_recorded_context_fields(self) -> RecordedContextFields {
        self.context_fields
    }

    #[inline]
    fn track_context_field(&mut self, field_name: &str) {
        match field_name {
            "request_id" => self.context_fields.has_request_id = true,
            "correlation_id" => self.context_fields.has_correlation_id = true,
            _ => {}
        }
    }
}

impl<LR: opentelemetry::logs::LogRecord> tracing::field::Visit for EventVisitor<'_, LR> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.track_context_field(field.name());
        if field.name() == "message" {
            self.log_record.set_body(AnyValue::from(format!("{:?}", value)));
        } else {
            self.log_record
                .add_attribute(Key::new(field.name()), AnyValue::from(format!("{:?}", value)));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.track_context_field(field.name());
        if field.name() == "message" {
            self.log_record.set_body(AnyValue::from(value.to_owned()));
        } else {
            self.log_record.add_attribute(Key::new(field.name()), AnyValue::from(value.to_owned()));
        }
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.track_context_field(field.name());
        self.log_record.add_attribute(Key::new(field.name()), AnyValue::from(value));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.track_context_field(field.name());
        // OTel AnyValue lacks u64; use i64 if in range, else string
        if value <= i64::MAX as u64 {
            self.log_record.add_attribute(Key::new(field.name()), AnyValue::from(value as i64));
        } else {
            self.log_record
                .add_attribute(Key::new(field.name()), AnyValue::from(value.to_string()));
        }
    }

    fn record_i128(&mut self, field: &tracing::field::Field, value: i128) {
        self.track_context_field(field.name());
        if value >= i64::MIN as i128 && value <= i64::MAX as i128 {
            self.log_record.add_attribute(Key::new(field.name()), AnyValue::from(value as i64));
        } else {
            self.log_record
                .add_attribute(Key::new(field.name()), AnyValue::from(value.to_string()));
        }
    }

    fn record_u128(&mut self, field: &tracing::field::Field, value: u128) {
        self.track_context_field(field.name());
        if value <= i64::MAX as u128 {
            self.log_record.add_attribute(Key::new(field.name()), AnyValue::from(value as i64));
        } else {
            self.log_record
                .add_attribute(Key::new(field.name()), AnyValue::from(value.to_string()));
        }
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.track_context_field(field.name());
        self.log_record.add_attribute(Key::new(field.name()), AnyValue::from(value));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.track_context_field(field.name());
        self.log_record.add_attribute(Key::new(field.name()), AnyValue::from(value));
    }

    fn record_error(
        &mut self, field: &tracing::field::Field, value: &(dyn std::error::Error + 'static),
    ) {
        self.track_context_field(field.name());
        self.log_record.add_attribute(Key::new(field.name()), AnyValue::from(value.to_string()));
    }
}

/// Stores request context on the current span and task-local fallback; sets OTel attributes.
/// No-op when both IDs are `None`.
pub fn set_request_context(request_id: Option<String>, correlation_id: Option<String>) {
    let request_ctx = RequestContext::new(request_id.clone(), correlation_id.clone());
    if !request_ctx.has_context() {
        return;
    }

    // Task-local fallback for when spans aren't connected (async-safe)
    set_task_request_context(request_ctx.clone());

    let span = Span::current();

    // OTel span attributes for trace correlation
    if let Some(ref req_id) = request_id {
        span.set_attribute("request_id", req_id.clone());
    }
    if let Some(ref corr_id) = correlation_id {
        span.set_attribute("correlation_id", corr_id.clone());
    }

    // Span extensions for log injection
    span.with_subscriber(|(id, dispatch)| {
        if let Some(with_ctx) = dispatch.downcast_ref::<WithRequestContext>() {
            with_ctx.set_context(dispatch, id, &request_ctx);
        }
    });
}

/// Sets request context in task-local storage (async-safe fallback).
fn set_task_request_context(ctx: RequestContext) {
    if let Some(task_id) = current_task_id() {
        TASK_CONTEXTS.insert(task_id, ctx);
    }
}

/// Gets request context from task-local storage.
pub(crate) fn get_task_request_context() -> Option<RequestContext> {
    let task_id = current_task_id()?;
    TASK_CONTEXTS.get(&task_id).map(|entry| entry.value().clone()).filter(|ctx| ctx.has_context())
}

/// Clears task-local request context at request boundaries.
pub fn clear_task_request_context() {
    if let Some(task_id) = current_task_id() {
        TASK_CONTEXTS.remove(&task_id);
    }
}

/// Returns the current tokio task ID, or None if not in a tokio runtime.
#[inline]
fn current_task_id() -> Option<tokio::task::Id> {
    tokio::task::try_id()
}

/// Helper for getting request context via `downcast_raw`.
pub(crate) struct GetRequestContext(fn(dispatch: &Dispatch, id: &Id) -> Option<RequestContext>);

impl GetRequestContext {
    pub(crate) fn get_context(&self, dispatch: &Dispatch, id: &Id) -> Option<RequestContext> {
        (self.0)(dispatch, id)
    }
}

/// Retrieves request context from span hierarchy, then task-local fallback.
pub fn get_request_context() -> Option<RequestContext> {
    // Try span hierarchy first
    let mut result = None;
    Span::current().with_subscriber(|(id, dispatch)| {
        if let Some(get_ctx) = dispatch.downcast_ref::<GetRequestContext>() {
            result = get_ctx.get_context(dispatch, id);
        }
    });

    if result.as_ref().is_some_and(|ctx| ctx.has_context()) {
        return result;
    }

    // Fall back to task-local storage
    get_task_request_context()
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_sdk::Resource;
    use opentelemetry_sdk::logs::LoggerProvider;
    use tracing_subscriber::Registry;
    use tracing_subscriber::layer::SubscriberExt;

    fn with_test_subscriber<F>(f: F)
    where
        F: FnOnce(),
    {
        let provider = LoggerProvider::builder().with_resource(Resource::empty()).build();
        let layer = ContextAwareOtelLogLayer::new(&provider);
        let subscriber = Registry::default().with(layer);

        tracing::subscriber::with_default(subscriber, f);
    }

    #[test]
    fn test_set_request_context_noop_when_empty() {
        with_test_subscriber(|| {
            let span = tracing::info_span!("test_span");
            let _guard = span.enter();
            set_request_context(None, None);
            assert!(get_request_context().is_none());
        });
    }

    #[test]
    fn test_get_request_context_returns_stored_values() {
        with_test_subscriber(|| {
            let span = tracing::info_span!("test_span");
            let _guard = span.enter();

            let initial = get_request_context();
            assert!(initial.is_none() || !initial.as_ref().is_some_and(|c| c.has_context()));

            let expected_req_id = "test-req-id-12345".to_string();
            let expected_corr_id = "test-corr-id-67890".to_string();
            set_request_context(Some(expected_req_id.clone()), Some(expected_corr_id.clone()));

            let retrieved = get_request_context();
            assert!(retrieved.is_some());
            let ctx = retrieved.expect("context should exist");
            assert_eq!(ctx.request_id, Some(expected_req_id));
            assert_eq!(ctx.correlation_id, Some(expected_corr_id));
        });
    }

    #[test]
    fn test_get_request_context_inherits_parent_context() {
        with_test_subscriber(|| {
            let parent_span = tracing::info_span!("parent");
            let _parent_guard = parent_span.enter();
            set_request_context(
                Some("parent-req-id".to_string()),
                Some("parent-corr-id".to_string()),
            );

            let child_span = tracing::info_span!("child");
            let _child_guard = child_span.enter();

            let child_ctx = get_request_context();
            assert!(child_ctx.is_some());
            assert_eq!(
                child_ctx.as_ref().and_then(|c| c.request_id.as_ref()),
                Some(&"parent-req-id".to_string())
            );
            assert_eq!(
                child_ctx.as_ref().and_then(|c| c.correlation_id.as_ref()),
                Some(&"parent-corr-id".to_string())
            );
        });
    }

    #[test]
    fn test_context_available_in_sibling_span_via_span_extensions() {
        // Context should not cross sibling spans.
        with_test_subscriber(|| {
            let span_a = tracing::info_span!("span_a");
            {
                let _guard = span_a.enter();
                set_request_context(
                    Some("span-a-req-id".to_string()),
                    Some("span-a-corr-id".to_string()),
                );

                let ctx = get_request_context();
                assert!(ctx.is_some());
                assert_eq!(ctx.as_ref().unwrap().request_id, Some("span-a-req-id".to_string()));
            }

            let span_b = tracing::info_span!("span_b");
            let _guard = span_b.enter();
            let ctx = get_request_context();
            assert!(ctx.is_none() || !ctx.as_ref().unwrap().has_context());
        });
    }

    #[test]
    fn test_context_cleaned_up_with_span() {
        // Context should not leak after leaving a span.
        with_test_subscriber(|| {
            {
                let span = tracing::info_span!("scoped_span");
                let _guard = span.enter();
                set_request_context(
                    Some("scoped-req-id".to_string()),
                    Some("scoped-corr-id".to_string()),
                );

                let ctx = get_request_context();
                assert!(ctx.is_some());
            }

            let new_span = tracing::info_span!("new_span");
            let _guard = new_span.enter();
            let ctx = get_request_context();
            assert!(ctx.is_none() || !ctx.as_ref().unwrap().has_context());
        });
    }

    #[test]
    fn test_log_emission_with_context_does_not_panic() {
        with_test_subscriber(|| {
            let span = tracing::info_span!("test_span");
            let _guard = span.enter();

            set_request_context(
                Some("test-req-id-12345".to_string()),
                Some("test-corr-id-67890".to_string()),
            );

            tracing::trace!("Trace level log");
            tracing::debug!("Debug level log");
            tracing::info!("Info level log");
            tracing::warn!("Warn level log");
            tracing::error!("Error level log");

            tracing::info!(custom_field = "custom_value", numeric_field = 42, "Log with fields");
        });
    }
}
