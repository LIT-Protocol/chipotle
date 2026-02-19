Observability concerns the lit-api-server and the lit-actions crates.

# Initialization
- A crate must be able to initialize at least standard stdout/err logging if it is not able to terminate the process
- Optionally, export of observability data via OpenTelemetry may be supported. Where OpenTelemetry support is indicated, the path to an OpenTelemetry Collector must be provided. If not, terminate the process. If the path is provided but not reachable (in case it is remote), log an error to stand out in continuous initialization.
- If open telemetry export is enabled, the crates export logs, metrics, and traces To a remotely available OpenTelemetry collector: The OpenTelemetry collector itself then forwards this data to Google Cloud Logging and observability.

# Data
- Each API request made by a user must be end-to-end traceable from request to response. For that purpose, a span with a random ID shall be created when an API request arrives and that ID shall be  propagated down the chain of calls until a response is returned to the user.
- The response shall contain the span ID of this request passed as a X-Request-Id http header with the response, such that a user can provide a span ID to us for receiving support. The X-Request-Id header MUST be attached to both success and failure responses.
- The user MAY pass an X-Request-ID header with an API request, but its value MUST be discarded/ignored and a random value ID must be used.
- A user MAY pass a X-Correlation-Id header along with their request. This ID is to similarly be propagated from request to response and to be returned to the user if present. It MUST NOT be attached as header on a response if the user did not provide a value for it in the request or if the header was missing in the request.
