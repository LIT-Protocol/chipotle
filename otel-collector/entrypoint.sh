#!/bin/sh
# OTel Collector sidecar entrypoint.
#
# Writes GCP service account credentials from the encrypted secret env var
# GCP_SERVICE_ACCOUNT_JSON (raw JSON or base64-encoded JSON) to a temp file,
# then sets GOOGLE_APPLICATION_CREDENTIALS before starting the collector.
#
# Required env vars:
#   GCP_SERVICE_ACCOUNT_JSON  - GCP service account key JSON (encrypted Phala secret)
#   GCP_PROJECT_ID            - GCP project ID (e.g. "my-gcp-project")

set -e

if [ -z "$GCP_SERVICE_ACCOUNT_JSON" ]; then
    echo "[otel-collector] ERROR: GCP_SERVICE_ACCOUNT_JSON is not set" >&2
    exit 1
fi

if [ -z "$GCP_PROJECT_ID" ]; then
    echo "[otel-collector] ERROR: GCP_PROJECT_ID is not set" >&2
    exit 1
fi

CREDS_FILE=/tmp/gcp-service-account.json

# Support both raw JSON (starts with '{') and base64-encoded JSON.
first_char=$(echo "$GCP_SERVICE_ACCOUNT_JSON" | head -c1)
if [ "$first_char" = "{" ]; then
    echo "$GCP_SERVICE_ACCOUNT_JSON" > "$CREDS_FILE"
else
    echo "$GCP_SERVICE_ACCOUNT_JSON" | base64 -d > "$CREDS_FILE"
fi

export GOOGLE_APPLICATION_CREDENTIALS="$CREDS_FILE"

echo "[otel-collector] Starting collector for GCP project: $GCP_PROJECT_ID"
exec /otelcol-contrib --config /etc/otelcol-contrib/config.yaml "$@"
