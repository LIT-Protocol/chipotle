use crate::PRIVACY_MODE_TAG;
use super::context_layer::get_request_context;

pub struct PrivacyModeLayer;

impl<S> tracing_subscriber::Layer<S> for PrivacyModeLayer
where
    S: tracing::Subscriber,
{
    fn enabled(
        &self, _metadata: &tracing::Metadata<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        if let Some(ctx) = get_request_context() {
            if let Some(ref request_id) = ctx.request_id {
                if request_id.contains(PRIVACY_MODE_TAG) {
                    return false;
                }
            }
        }
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
