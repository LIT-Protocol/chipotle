# Minimal any-language Lit Action bundled with `lit-bundle`.
# The sandbox runs `bash startup.sh`; top-level jsParams arrive as env vars.
#
#   lit-bundle bundle   examples/shell            # → prints the CID
#   lit-bundle deploy   examples/shell --name shell-demo
#   lit-bundle run      examples/shell --param name=world

lit print "hello from a bundled shell action"
lit print "params: $(lit params)"

lit set-response "{\"ok\": true, \"greeted\": \"${name:-anon}\"}"
