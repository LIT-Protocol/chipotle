# Minimal any-language Lit Action: the sandbox always executes
# `bash startup.sh` — this file, from the bundle root (or a script sent
# with the request, which takes precedence). No manifest needed.

lit print "hello from a shell action"

# Params arrive as JSON via the CLI, and top-level params are also injected
# as environment variables; anything on stdout/stderr lands in the logs.
lit params

lit set-response '{"ok": true, "lang": "sh"}'
