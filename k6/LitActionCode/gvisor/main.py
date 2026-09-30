"""gVisor deployment smoke test action.

Proves that a recent Python and a `pip install` both work inside the
any-language sandbox. Talks to the host via the preinstalled `lit` CLI - the
same op-loop path `/lit_binary_action` drives in production.
"""

import json
import subprocess
import sys

import cowsay  # installed by startup.sh; importing it proves the pip install worked


def lit(*args, stdin=None):
    """Call the guest `lit` CLI, returning stdout without the trailing newline."""
    out = subprocess.run(
        ["lit", *args], input=stdin, capture_output=True, text=True, check=True
    )
    return out.stdout.rstrip("\n")


params = json.loads(lit("params") or "null") or {}
name = params.get("name", "world")
python_version = "{}.{}.{}".format(*sys.version_info[:3])
greeting = f"hello {name} from python {python_version}"

# cowsay's art lands in the action logs (the sandbox forwards stdout as logs).
lit("print", cowsay.get_output_string("cow", greeting))

lit(
    "set-response",
    stdin=json.dumps(
        {
            "ok": True,
            "greeting": greeting,
            "python": python_version,
            "cowsay": cowsay.__version__,
        }
    ),
)
