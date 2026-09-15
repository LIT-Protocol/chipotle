# Minimal any-language Lit Action. `lit run` executes exactly what the
# sandbox executes — `bash startup.sh` — so this runs unchanged in the TEE.
# Locally, `lit` resolves the ops against your local master key.

lit print "hello from a shell action"

# Params arrive as JSON via the CLI, and top-level params are also injected
# as environment variables; anything on stdout/stderr lands in the logs.
lit print "params: $(lit params)"

# Derive a key for a permitted PKP wallet and "sign" with it (a real action
# would feed the raw key to a signing library). PKP_ID may come straight
# from js-params (e.g. {"PKP_ID": "my-pkp"} in lit.job.json).
KEY=$(lit get-private-key "${PKP_ID:-example-pkp}")
lit print "derived a ${#KEY}-char key"

lit set-response "{\"ok\": true, \"lang\": \"sh\"}"
