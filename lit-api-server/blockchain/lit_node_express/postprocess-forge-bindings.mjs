#!/usr/bin/env node
/**
 * Post-process `forge bind` output for lit-api-server AccountConfig bindings.
 *
 * `forge bind --module --single-file` writes a standalone `mod.rs` that expects
 * an `alloy_contract` crate path. lit-api-server depends on the umbrella
 * `alloy` crate, so this inserts a local alias and preserves the small
 * compatibility re-exports used by the accounts module.
 */

import fs from "fs";
import path from "path";

const AUTOGEN_MARKER = "// These files may be overwritten by the codegen system at any time.\n";
const ALIAS = "use alloy::contract as alloy_contract;\n\n";
const RE_EXPORTS = `

pub use AccountConfig::AccountConfigErrors;
pub use AppStorage::{Metadata, PkpData};
pub use ViewsFacet::{KeyValueReturn, UsageApiKeyReturn};
`;

function parseArgs(argv) {
  const args = {};

  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];
    if (!arg.startsWith("--")) {
      throw new Error(`unexpected positional argument: ${arg}`);
    }

    const eq = arg.indexOf("=");
    if (eq !== -1) {
      args[arg.slice(2, eq)] = arg.slice(eq + 1);
      continue;
    }

    const key = arg.slice(2);
    const value = argv[++i];
    if (!value || value.startsWith("--")) {
      throw new Error(`missing value for --${key}`);
    }
    args[key] = value;
  }

  for (const key of ["forge-output", "binding-output", "abi-source", "abi-output"]) {
    if (!args[key]) {
      throw new Error(`missing required argument --${key}`);
    }
  }

  return args;
}

function stripRustDocComments(source) {
  // `cargo test` runs doctests from checked-in Rust sources. The forge output
  // contains generated prose doc blocks with markdown/backticks that rustdoc
  // currently mis-detects as doctest snippets, so keep the comments for humans
  // but make them non-doc comments.
  return source
    .split(/(?<=\n)/)
    .map((line) => {
      const indent = line.match(/^\s*/)[0];
      const stripped = line.slice(indent.length);

      if (stripped.startsWith("//!") || stripped.startsWith("///")) {
        return `${indent}//${stripped.slice(3)}`;
      }
      if (stripped.startsWith("/**") || stripped.startsWith("/*!")) {
        return `${indent}/*${stripped.slice(3)}`;
      }
      return line;
    })
    .join("");
}

function main() {
  const args = parseArgs(process.argv);

  let generated = fs.readFileSync(args["forge-output"], "utf8");
  generated = stripRustDocComments(generated);

  if (!generated.includes(ALIAS)) {
    if (!generated.includes(AUTOGEN_MARKER)) {
      throw new Error("unexpected forge bind output: autogen marker not found");
    }
    generated = generated.replace(AUTOGEN_MARKER, AUTOGEN_MARKER + ALIAS);
  }

  if (!generated.includes("pub use AccountConfig::AccountConfigErrors;")) {
    generated = generated.trimEnd() + RE_EXPORTS;
  }

  fs.mkdirSync(path.dirname(args["binding-output"]), { recursive: true });
  fs.writeFileSync(args["binding-output"], generated);

  fs.mkdirSync(path.dirname(args["abi-output"]), { recursive: true });
  fs.copyFileSync(args["abi-source"], args["abi-output"]);
}

try {
  main();
} catch (err) {
  console.error(err.message);
  process.exit(1);
}
