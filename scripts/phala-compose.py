#!/usr/bin/env python3
"""Select the dstack-ingress generation for a lane's compose file.

    phala-compose.py <1|2> [compose] [ingress-v2 block] > out.yml

docker-compose.phala.yml carries the 1.x ingress between BEGIN/END markers.
Generation 1 keeps it; generation 2 swaps in docker-compose.phala.ingress-v2.yml.
The markers are dropped either way. Everything else passes through untouched,
byte for byte, so the placeholders the deploy workflows substitute survive.
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BEGIN = "  # BEGIN dstack-ingress"
END = "  # END dstack-ingress"


def render(generation, compose, v2_block):
    if generation not in ("1", "2"):
        raise ValueError(f"ingress generation must be 1 or 2, got {generation!r}")
    lines = compose.split("\n")
    begins = [i for i, line in enumerate(lines) if line.startswith(BEGIN)]
    ends = [i for i, line in enumerate(lines) if line.startswith(END)]
    if len(begins) != 1 or len(ends) != 1 or begins[0] >= ends[0]:
        raise ValueError("compose file must contain exactly one BEGIN/END dstack-ingress block")
    block = lines[begins[0] + 1:ends[0]] if generation == "1" else v2_block.rstrip("\n").split("\n")
    if not any(line == "  dstack-ingress:" for line in block):
        raise ValueError("ingress block does not define the dstack-ingress service")
    return "\n".join(lines[:begins[0]] + block + lines[ends[0] + 1:])


def main(argv):
    if len(argv) < 2:
        sys.exit(__doc__)
    compose = Path(argv[2]) if len(argv) > 2 else ROOT / "docker-compose.phala.yml"
    v2 = Path(argv[3]) if len(argv) > 3 else ROOT / "docker-compose.phala.ingress-v2.yml"
    try:
        sys.stdout.write(render(argv[1], compose.read_text(), v2.read_text()))
    except ValueError as error:
        sys.exit(f"::error::{error}")


if __name__ == "__main__":
    main(sys.argv)
