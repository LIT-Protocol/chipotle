#!/bin/bash

# Start the lit-actions binary in the background
/usr/local/bin/lit_actions &

# Start the lit-api-server binary in the foreground
exec /usr/local/bin/lit-api-server

# Note: Using 'exec' ensures that the second binary receives
# signals (like SIGTERM) sent to the container's main process (PID 1).
