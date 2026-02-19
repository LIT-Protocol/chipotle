use crate::PRIVACY_MODE_TAG;

pub struct PrivacyModeLayer;

impl<S> tracing_subscriber::Layer<S> for PrivacyModeLayer
where
    S: tracing::Subscriber,
{
    fn enabled(
        &self, _metadata: &tracing::Metadata<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        // RADICAL SIMPLIFICATION:
        // Privacy filtering in the previous architecture was tied to a complex request context
        // that is not yet fully available in this simplified OTLP-as-sidecar world.
        // For now, we remain transparent but provide the placeholder for the layer.
        true
    }

    fn on_event(
        &self, _event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
    }

    fn on_new_span(
        &self, _attrs: &tracing::span::Attributes<'_>, _id: &tracing::span::Id,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
    }
}
