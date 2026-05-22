#!/usr/bin/env python3
"""Post-process `forge bind` output for lit-api-server AccountConfig bindings.

`forge bind --module --single-file` writes a standalone `mod.rs` that expects an
`alloy_contract` crate path. lit-api-server depends on the umbrella `alloy`
crate, so this inserts a local alias and preserves the small compatibility
re-exports used by the accounts module.
"""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path


AUTOGEN_MARKER = "// These files may be overwritten by the codegen system at any time.\n"
ALIAS = "use alloy::contract as alloy_contract;\n\n"
RE_EXPORTS = """

pub use AccountConfig::AccountConfigErrors;
pub use AppStorage::{Metadata, PkpData};
pub use ViewsFacet::{KeyValueReturn, UsageApiKeyReturn};
"""


def strip_rust_doc_comments(source: str) -> str:
    """Convert forge doc comments to regular comments.

    `cargo test` runs doctests from checked-in Rust sources. The forge output
    contains generated prose doc blocks with markdown/backticks that rustdoc
    currently mis-detects as doctest snippets, so keep the comments for humans
    but make them non-doc comments.
    """
    out: list[str] = []
    for line in source.splitlines(keepends=True):
        stripped = line.lstrip()
        indent = line[: len(line) - len(stripped)]
        if stripped.startswith("//!") or stripped.startswith("///"):
            out.append(indent + "//" + stripped[3:])
        elif stripped.startswith("/**") or stripped.startswith("/*!"):
            out.append(indent + "/*" + stripped[3:])
        else:
            out.append(line)
    return "".join(out)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--forge-output", required=True, type=Path)
    parser.add_argument("--binding-output", required=True, type=Path)
    parser.add_argument("--abi-source", required=True, type=Path)
    parser.add_argument("--abi-output", required=True, type=Path)
    args = parser.parse_args()

    generated = args.forge_output.read_text()
    generated = strip_rust_doc_comments(generated)

    if ALIAS not in generated:
        if AUTOGEN_MARKER not in generated:
            raise SystemExit("unexpected forge bind output: autogen marker not found")
        generated = generated.replace(AUTOGEN_MARKER, AUTOGEN_MARKER + ALIAS, 1)

    if "pub use AccountConfig::AccountConfigErrors;" not in generated:
        generated = generated.rstrip() + RE_EXPORTS

    args.binding_output.parent.mkdir(parents=True, exist_ok=True)
    args.binding_output.write_text(generated)

    args.abi_output.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(args.abi_source, args.abi_output)


if __name__ == "__main__":
    main()
