# Minimal any-language Lit Action: plain POSIX shell.
# (string-form entrypoint is run as `sh run.sh` — no exec bit needed)

lit print "hello from a shell action"

# Params arrive as JSON; anything on stdout/stderr also lands in the logs.
lit params

lit set-response '{"ok": true, "lang": "sh"}'
