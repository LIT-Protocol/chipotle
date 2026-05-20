#!/bin/sh
# Entrypoint for lit-actions: chown the shared socket volume to the lit user,
# then drop privileges and exec the server binary.
#
# Why root-then-drop instead of plain USER lit in the Dockerfile:
# docker-compose mounts the `lit-socket` named volume at /tmp so lit-actions
# and lit-api-server can share /tmp/lit_actions.sock. Named volumes mount as
# root:root on first creation (Docker only copies image perms when the image
# path has *content*; an empty /tmp dir doesn't carry its 1777 mode through).
# With USER lit set up front, lit_actions can't write to /tmp and the
# `UnixListener::bind` in lit-actions/grpc/unix.rs panics — which is what
# made the /health probe report `lit_actions_reachable: false` on Phala
# after CPL-300 dropped root in both containers.
#
# The chown is idempotent and only touches /tmp inside this container; the
# matching socket file is set to 0o777 by start_server so the connecting
# UID (lit-api-server's lit user) doesn't matter. setpriv drops to the lit
# user (uid 10001) before exec, matching the security posture from CPL-300.

set -e

chown lit:lit /tmp

# Match the env a `USER lit` Dockerfile directive would have set up — setpriv
# only changes uid/gid, not HOME/USER.
exec env HOME=/home/lit USER=lit \
    setpriv --reuid=10001 --regid=10001 --init-groups --no-new-privs "$@"
