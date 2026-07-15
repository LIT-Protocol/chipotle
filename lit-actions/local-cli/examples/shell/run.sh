# Minimal any-language Lit Action: plain POSIX shell.
# Identical to the gvisor-server shell example — it runs unchanged in the
# TEE. Locally, `lit` resolves the ops against your local master key.

lit print "hello from a shell action"

# Params arrive as JSON; anything on stdout/stderr also lands in the logs.
lit print "params: $(lit params)"

# Derive a key for a permitted PKP wallet and "sign" with it (a real action
# would feed the raw key to a signing library).
KEY=$(lit get-private-key "${PKP_ID:-example-pkp}")
lit print "derived a ${#KEY}-char key"

lit set-response "{\"ok\": true, \"lang\": \"sh\"}"
