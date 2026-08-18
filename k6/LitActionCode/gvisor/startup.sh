# gVisor deployment smoke test: prove a recent Python plus a `pip install`
# both work inside the any-language sandbox. The sandbox only ever runs
# `bash startup.sh` (CPL-355); launching the interpreter is this script's job.
# Top-level js-params are already in the environment.
set -eu

python3 --version

# Debian's system Python is externally-managed (PEP 668). This is a throwaway,
# per-execution sandbox, so install straight into it rather than a venv. Pin
# the version so the gate stays deterministic across deploys.
python3 -m pip install --quiet --break-system-packages --root-user-action=ignore cowsay==6.1

python3 main.py
