#!/bin/sh
# Entrypoint for lit-actions: ensure the shared socket volume is writable,
# then drop privileges and exec the server binary.
#
# Why root-then-drop instead of plain USER lit in the Dockerfile:
# docker-compose mounts the `lit-socket` named volume at /tmp so lit-actions
# and lit-api-server can share /tmp/lit_actions.sock. Named volumes mount as
# root:root 0755 on first creation (Docker only copies image perms when the
# image path has *content*; an empty /tmp dir doesn't carry its 1777 mode
# through). With USER lit set up front, lit_actions can't write to /tmp and
# the `UnixListener::bind` in lit-actions/grpc/unix.rs panics — which is
# what made the /health probe report `lit_actions_reachable: false` on
# Phala after CPL-300 dropped root in both containers.
#
# We restore the standard 1777 mode rather than chowning, so /tmp keeps the
# world-writable + sticky semantics any debian process expects. /tmp is the
# shared `lit-socket` volume, so the chmod is visible to lit-api-server as
# well — that's fine: lit-api-server only reads/connects to the socket, and
# the socket file itself is set to 0o777 by start_server so connecting UID
# doesn't matter. The chmod is idempotent across container restarts.
#
# setpriv drops to the lit user (uid 10001) before exec, matching the
# security posture from CPL-300.

set -e

chmod 1777 /tmp

# Match the env a `USER lit` Dockerfile directive would have set up — setpriv
# only changes uid/gid, not HOME/USER.
exec env HOME=/home/lit USER=lit \
    setpriv --reuid=10001 --regid=10001 --init-groups --no-new-privs "$@"
