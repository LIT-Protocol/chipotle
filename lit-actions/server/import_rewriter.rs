//! Rewrites static ES module `import` statements into dynamic `import()` calls.
//!
//! The Deno runtime executes user code in **script mode** via `execute_script()`,
//! which does not support static `import` declarations (an ES module feature).
//! This module scans user code for static imports that appear before
//! `async function main`, strips them from the source, and returns structured
//! data that the runtime uses to generate equivalent dynamic `import()` calls
//! inside the async wrapper.

/// A single binding from an import statement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ImportBinding {
    /// `import foo from "..."`
    Default(String),
    /// `import { a }` or `import { a as b }`
    Named { imported: String, local: String },
    /// `import * as foo from "..."`
    Namespace(String),
}

/// A parsed ES module import statement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedImport {
    /// The module specifier (the string inside quotes after `from`).
    pub specifier: String,
    /// The import bindings (empty for side-effect-only imports).
    pub bindings: Vec<ImportBinding>,
}

/// Result of rewriting imports in user code.
pub(crate) struct RewriteResult {
    /// The user code with import statements removed.
    pub code: String,
    /// The parsed imports in order of appearance.
    pub imports: Vec<ParsedImport>,
}

/// Scan user code for static `import` statements before `async function main`,
/// remove them, and return the parsed imports.
///
/// Code after `async function main` (and any non-import code before it) is
/// preserved unchanged. The scanner is aware of single-line comments (`//`),
/// block comments (`/* */`), and string/template literals so that import-like
/// text inside those constructs is never mistakenly rewritten.
pub(crate) fn rewrite_imports(code: &str) -> RewriteResult {
    let main_pos = find_main_declaration(code);
    let preamble = &code[..main_pos];
    let bytes = preamble.as_bytes();

    let mut imports = Vec::new();
    let mut ranges_to_remove: Vec<(usize, usize)> = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        let b = bytes[pos];

        // Skip single-line comments
        if b == b'/' && bytes.get(pos + 1) == Some(&b'/') {
            match preamble[pos..].find('\n') {
                Some(nl) => {
                    pos += nl + 1;
                    continue;
                }
                None => break,
            }
        }

        // Skip block comments
        if b == b'/' && bytes.get(pos + 1) == Some(&b'*') {
            match preamble[pos + 2..].find("*/") {
                Some(end) => {
                    pos += end + 4;
                    continue;
                }
                None => break,
            }
        }

        // Skip string literals
        if b == b'"' || b == b'\'' {
            if let Some(len) = skip_string_literal(&preamble[pos..], b) {
                pos += len;
                continue;
            }
            pos += 1;
            continue;
        }

        // Skip template literals
        if b == b'`' {
            if let Some(len) = skip_template_literal(&preamble[pos..]) {
                pos += len;
                continue;
            }
            pos += 1;
            continue;
        }

        // Check for `import` keyword at a statement boundary
        if b == b'i'
            && preamble[pos..].starts_with("import")
            && !is_ident_byte(bytes.get(pos + 6).copied())
            && is_line_start(preamble, pos)
            && let Some((imp, consumed)) = parse_import_statement(&preamble[pos..])
        {
            let start = pos;
            let mut end = pos + consumed;
            // Consume trailing horizontal whitespace + one newline
            end += count_horizontal_ws(&preamble[end..]);
            if bytes.get(end) == Some(&b'\n') {
                end += 1;
            }
            ranges_to_remove.push((start, end));
            imports.push(imp);
            pos = end;
            continue;
        }

        pos += 1;
    }

    // Rebuild code with import statements removed
    let mut result = String::with_capacity(code.len());
    let mut last = 0;
    for &(start, end) in &ranges_to_remove {
        result.push_str(&code[last..start]);
        last = end;
    }
    result.push_str(&code[last..]);

    RewriteResult {
        code: result,
        imports,
    }
}

/// Check if `pos` is at the start of a line (only whitespace between the
/// previous newline and `pos`).
fn is_line_start(s: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let before = &s[..pos];
    for b in before.bytes().rev() {
        match b {
            b'\n' => return true,
            b' ' | b'\t' | b'\r' => continue,
            _ => return false,
        }
    }
    true // beginning of string
}

/// Find the byte offset of the real `async function main` declaration,
/// skipping occurrences inside comments, strings, and template literals.
/// Also requires `main` to be followed by a non-identifier character
/// (rejects `main2`, `mainHelper`, etc.).
fn find_main_declaration(code: &str) -> usize {
    const NEEDLE: &[u8] = b"async function main";
    let bytes = code.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() {
        // Skip single-line comments
        if bytes[pos] == b'/' && bytes.get(pos + 1) == Some(&b'/') {
            match code[pos..].find('\n') {
                Some(nl) => {
                    pos += nl + 1;
                    continue;
                }
                None => return code.len(),
            }
        }

        // Skip block comments
        if bytes[pos] == b'/' && bytes.get(pos + 1) == Some(&b'*') {
            match code[pos + 2..].find("*/") {
                Some(end) => {
                    pos += end + 4;
                    continue;
                }
                None => return code.len(),
            }
        }

        // Skip string literals
        if bytes[pos] == b'"' || bytes[pos] == b'\'' {
            if let Some(len) = skip_string_literal(&code[pos..], bytes[pos]) {
                pos += len;
                continue;
            }
            pos += 1;
            continue;
        }

        // Skip template literals
        if bytes[pos] == b'`' {
            if let Some(len) = skip_template_literal(&code[pos..]) {
                pos += len;
                continue;
            }
            pos += 1;
            continue;
        }

        // Check for the needle at this position
        if bytes[pos..].starts_with(NEEDLE)
            && !is_ident_byte(bytes.get(pos + NEEDLE.len()).copied())
        {
            return pos;
        }

        pos += 1;
    }

    code.len()
}

/// Generate JavaScript code that performs dynamic `import()` calls and
/// destructures the results into local `const` bindings.
///
/// Each import becomes one or two statements:
/// - Side-effect: `await import("specifier");`
/// - Named/default/namespace: `const { a, b: c, default: D } = await import("specifier");`
/// - Namespace: `const Mod = await import("specifier");`
pub(crate) fn generate_dynamic_imports(imports: &[ParsedImport]) -> String {
    let mut out = String::new();
    for imp in imports {
        let spec = js_escape(&imp.specifier);

        if imp.bindings.is_empty() {
            // Side-effect import
            out.push_str(&format!("await import(\"{spec}\");\n"));
            continue;
        }

        // Check if there's exactly one namespace binding (no destructuring needed)
        if imp.bindings.len() == 1
            && let ImportBinding::Namespace(ref name) = imp.bindings[0]
        {
            out.push_str(&format!("const {name} = await import(\"{spec}\");\n"));
            continue;
        }

        // Destructuring: const { ... } = await import("...");
        // When both a namespace and named/default bindings exist (e.g.
        // `import Default, * as NS from "..."`), import once into a temp
        // variable and assign both from it to avoid a redundant fetch.
        let mut parts = Vec::new();
        let mut has_namespace = false;
        let mut ns_name = String::new();

        for binding in &imp.bindings {
            match binding {
                ImportBinding::Default(name) => {
                    if name == "default" {
                        parts.push("default".to_string());
                    } else {
                        parts.push(format!("default: {name}"));
                    }
                }
                ImportBinding::Named { imported, local } => {
                    if imported == local {
                        parts.push(imported.clone());
                    } else {
                        parts.push(format!("{imported}: {local}"));
                    }
                }
                ImportBinding::Namespace(name) => {
                    has_namespace = true;
                    ns_name = name.clone();
                }
            }
        }

        if has_namespace && !parts.is_empty() {
            // Mixed namespace + named/default: import once, assign both
            out.push_str(&format!("const {ns_name} = await import(\"{spec}\");\n"));
            out.push_str(&format!("const {{ {} }} = {ns_name};\n", parts.join(", "),));
        } else if !parts.is_empty() {
            out.push_str(&format!(
                "const {{ {} }} = await import(\"{spec}\");\n",
                parts.join(", "),
            ));
        } else if has_namespace {
            out.push_str(&format!("const {ns_name} = await import(\"{spec}\");\n"));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Escape a string for embedding inside a JS double-quoted string literal.
/// Handles backslashes, double quotes, and JS line terminators.
fn js_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(ch),
        }
    }
    out
}

fn is_ident_byte(b: Option<u8>) -> bool {
    matches!(b, Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b'$')
}

fn count_horizontal_ws(s: &str) -> usize {
    s.bytes().take_while(|b| *b == b' ' || *b == b'\t').count()
}

/// Skip past a string literal starting with the given quote byte.
/// Returns the number of bytes consumed (including both quotes), or None if unterminated.
fn skip_string_literal(s: &str, quote: u8) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&quote) {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2; // skip escaped character
        } else if bytes[i] == quote {
            return Some(i + 1);
        } else {
            i += 1;
        }
    }
    None // unterminated
}

/// Skip past a template literal (backtick string).
/// Handles escaped backticks but does not recurse into `${...}` expressions.
/// Returns the number of bytes consumed (including both backticks), or None if unterminated.
fn skip_template_literal(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'`') {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2; // skip escaped character
        } else if bytes[i] == b'`' {
            return Some(i + 1);
        } else {
            i += 1;
        }
    }
    None // unterminated
}

/// Parse a single import statement starting at the `import` keyword.
/// Returns `(ParsedImport, bytes_consumed)` or `None` if parsing fails.
fn parse_import_statement(s: &str) -> Option<(ParsedImport, usize)> {
    debug_assert!(s.starts_with("import"));
    let mut cur = Cursor::new(s);
    cur.advance(6); // skip "import"
    cur.skip_ws();

    let rest = cur.remaining();

    // Side-effect import: import "specifier" / import 'specifier'
    if rest.starts_with('"') || rest.starts_with('\'') {
        let spec = cur.parse_string()?;
        cur.skip_ws();
        cur.eat(b';');
        return Some((
            ParsedImport {
                specifier: spec,
                bindings: vec![],
            },
            cur.pos,
        ));
    }

    let mut bindings = Vec::new();

    if rest.starts_with('{') {
        // Named imports: import { a, b as c } from "..."
        bindings.extend(cur.parse_named_imports()?);
    } else if rest.starts_with('*') {
        // Namespace import: import * as Name from "..."
        bindings.push(ImportBinding::Namespace(cur.parse_namespace()?));
    } else {
        // Default import: import Default from "..."
        // Optionally followed by , { ... } or , * as ...
        let name = cur.parse_ident()?;
        bindings.push(ImportBinding::Default(name));
        cur.skip_ws();

        if cur.eat(b',') {
            cur.skip_ws();
            let rest2 = cur.remaining();
            if rest2.starts_with('{') {
                bindings.extend(cur.parse_named_imports()?);
            } else if rest2.starts_with('*') {
                bindings.push(ImportBinding::Namespace(cur.parse_namespace()?));
            }
        }
    }

    // Expect `from`
    cur.skip_ws();
    if !cur.remaining().starts_with("from") {
        return None;
    }
    cur.advance(4);
    cur.skip_ws();

    let specifier = cur.parse_string()?;
    cur.skip_ws();
    cur.eat(b';');

    Some((
        ParsedImport {
            specifier,
            bindings,
        },
        cur.pos,
    ))
}

/// Simple cursor for parsing import statement text.
struct Cursor<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn remaining(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn advance(&mut self, n: usize) {
        self.pos += n;
    }

    fn skip_ws(&mut self) {
        let r = self.remaining();
        self.pos += r.len() - r.trim_start().len();
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.src.as_bytes().get(self.pos) == Some(&byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Parse a quoted string (single or double) and return its contents.
    /// Handles escape sequences so that `\"` inside a string does not
    /// terminate the parse early.
    fn parse_string(&mut self) -> Option<String> {
        let r = self.remaining();
        let bytes = r.as_bytes();
        let quote = *bytes.first()?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2; // skip escaped character
            } else if bytes[i] == quote {
                let value = r[1..i].to_string();
                self.pos += i + 1;
                return Some(value);
            } else {
                i += 1;
            }
        }
        None // unterminated string
    }

    /// Parse a JavaScript identifier.
    fn parse_ident(&mut self) -> Option<String> {
        let r = self.remaining();
        let len = r
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'$')
            .count();
        if len == 0 {
            return None;
        }
        let ident = r[..len].to_string();
        self.pos += len;
        Some(ident)
    }

    /// Parse `{ a, b as c, d }` returning the named import bindings.
    fn parse_named_imports(&mut self) -> Option<Vec<ImportBinding>> {
        if !self.eat(b'{') {
            return None;
        }
        let mut bindings = Vec::new();
        loop {
            self.skip_ws();
            if self.eat(b'}') {
                break;
            }
            let imported = self.parse_ident()?;
            self.skip_ws();

            let local = if self.remaining().starts_with("as ")
                || self.remaining().starts_with("as\t")
                || self.remaining().starts_with("as\n")
            {
                self.advance(2); // skip "as"
                self.skip_ws();
                self.parse_ident()?
            } else {
                imported.clone()
            };
            bindings.push(ImportBinding::Named { imported, local });

            self.skip_ws();
            if !self.eat(b',') {
                self.skip_ws();
                self.eat(b'}');
                break;
            }
        }
        Some(bindings)
    }

    /// Parse `* as Name` and return the local name.
    fn parse_namespace(&mut self) -> Option<String> {
        if !self.eat(b'*') {
            return None;
        }
        self.skip_ws();
        // Expect "as" followed by whitespace (same boundary check as parse_named_imports)
        let r = self.remaining();
        if !(r.starts_with("as ") || r.starts_with("as\t") || r.starts_with("as\n")) {
            return None;
        }
        self.advance(2);
        self.skip_ws();
        self.parse_ident()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_import() {
        let code = "import { z } from \"zod@3.22.4/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm");
        assert_eq!(
            result.imports[0].bindings,
            vec![ImportBinding::Named {
                imported: "z".into(),
                local: "z".into()
            }]
        );
        assert_eq!(result.code, "async function main() {}");
    }

    #[test]
    fn multiple_named_imports() {
        let code =
            "import { encode, decode } from \"cbor-x@1.5.9/+esm\";\n\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "cbor-x@1.5.9/+esm");
        assert_eq!(
            result.imports[0].bindings,
            vec![
                ImportBinding::Named {
                    imported: "encode".into(),
                    local: "encode".into()
                },
                ImportBinding::Named {
                    imported: "decode".into(),
                    local: "decode".into()
                },
            ]
        );
        assert_eq!(result.code, "\nasync function main() {}");
    }

    #[test]
    fn renamed_import() {
        let code = "import { foo as bar } from \"pkg@1.0.0/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(
            result.imports[0].bindings,
            vec![ImportBinding::Named {
                imported: "foo".into(),
                local: "bar".into()
            }]
        );
    }

    #[test]
    fn default_import() {
        let code = "import Ajv from \"ajv@8.12.0/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "ajv@8.12.0/+esm");
        assert_eq!(
            result.imports[0].bindings,
            vec![ImportBinding::Default("Ajv".into())]
        );
    }

    #[test]
    fn namespace_import() {
        let code = "import * as Mod from \"pkg@1.0.0/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(
            result.imports[0].bindings,
            vec![ImportBinding::Namespace("Mod".into())]
        );
    }

    #[test]
    fn default_and_named() {
        let code =
            "import Default, { a, b as c } from \"pkg@1.0.0/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(
            result.imports[0].bindings,
            vec![
                ImportBinding::Default("Default".into()),
                ImportBinding::Named {
                    imported: "a".into(),
                    local: "a".into()
                },
                ImportBinding::Named {
                    imported: "b".into(),
                    local: "c".into()
                },
            ]
        );
    }

    #[test]
    fn side_effect_import() {
        let code = "import \"side-effect@1.0.0/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "side-effect@1.0.0/+esm");
        assert!(result.imports[0].bindings.is_empty());
    }

    #[test]
    fn single_quoted_specifier() {
        let code = "import { a } from 'pkg@1.0.0/+esm';\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports[0].specifier, "pkg@1.0.0/+esm");
    }

    #[test]
    fn multi_line_named_imports() {
        let code =
            "import {\n  a,\n  b,\n  c\n} from \"pkg@1.0.0/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(
            result.imports[0].bindings,
            vec![
                ImportBinding::Named {
                    imported: "a".into(),
                    local: "a".into()
                },
                ImportBinding::Named {
                    imported: "b".into(),
                    local: "b".into()
                },
                ImportBinding::Named {
                    imported: "c".into(),
                    local: "c".into()
                },
            ]
        );
    }

    #[test]
    fn multiple_imports() {
        let code = "\
import { z } from \"zod@3.22.4/+esm\";
import { format } from \"date-fns@3.6.0/+esm\";

async function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 2);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm");
        assert_eq!(result.imports[1].specifier, "date-fns@3.6.0/+esm");
        assert_eq!(result.code, "\nasync function main() {}");
    }

    #[test]
    fn inline_integrity_hash() {
        let code = "import { z } from \"zod@3.22.4/+esm#sha384-abc123\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm#sha384-abc123");
    }

    #[test]
    fn full_cdn_url() {
        let code = "import { z } from \"https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(
            result.imports[0].specifier,
            "https://cdn.jsdelivr.net/npm/zod@3.22.4/+esm"
        );
    }

    #[test]
    fn no_imports() {
        let code = "async function main() { return 42; }";
        let result = rewrite_imports(code);
        assert!(result.imports.is_empty());
        assert_eq!(result.code, code);
    }

    #[test]
    fn code_before_imports_preserved() {
        let code = "// comment\nimport { z } from \"zod@3.22.4/+esm\";\n\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.code, "// comment\n\nasync function main() {}");
    }

    #[test]
    fn imports_after_main_not_touched() {
        let code = "async function main() {\n  // import { z } from \"zod@3.22.4/+esm\";\n}";
        let result = rewrite_imports(code);
        assert!(result.imports.is_empty());
        assert_eq!(result.code, code);
    }

    #[test]
    fn no_semicolon() {
        let code = "import { z } from \"zod@3.22.4/+esm\"\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm");
    }

    #[test]
    fn generate_named_dynamic_import() {
        let imports = vec![ParsedImport {
            specifier: "zod@3.22.4/+esm".into(),
            bindings: vec![ImportBinding::Named {
                imported: "z".into(),
                local: "z".into(),
            }],
        }];
        let code = generate_dynamic_imports(&imports);
        assert_eq!(code, "const { z } = await import(\"zod@3.22.4/+esm\");\n");
    }

    #[test]
    fn generate_default_dynamic_import() {
        let imports = vec![ParsedImport {
            specifier: "ajv@8.12.0/+esm".into(),
            bindings: vec![ImportBinding::Default("Ajv".into())],
        }];
        let code = generate_dynamic_imports(&imports);
        assert_eq!(
            code,
            "const { default: Ajv } = await import(\"ajv@8.12.0/+esm\");\n"
        );
    }

    #[test]
    fn generate_namespace_dynamic_import() {
        let imports = vec![ParsedImport {
            specifier: "pkg@1.0.0/+esm".into(),
            bindings: vec![ImportBinding::Namespace("Mod".into())],
        }];
        let code = generate_dynamic_imports(&imports);
        assert_eq!(code, "const Mod = await import(\"pkg@1.0.0/+esm\");\n");
    }

    #[test]
    fn generate_side_effect_dynamic_import() {
        let imports = vec![ParsedImport {
            specifier: "side-effect@1.0.0/+esm".into(),
            bindings: vec![],
        }];
        let code = generate_dynamic_imports(&imports);
        assert_eq!(code, "await import(\"side-effect@1.0.0/+esm\");\n");
    }

    #[test]
    fn generate_renamed_dynamic_import() {
        let imports = vec![ParsedImport {
            specifier: "pkg@1.0.0/+esm".into(),
            bindings: vec![ImportBinding::Named {
                imported: "foo".into(),
                local: "bar".into(),
            }],
        }];
        let code = generate_dynamic_imports(&imports);
        assert_eq!(
            code,
            "const { foo: bar } = await import(\"pkg@1.0.0/+esm\");\n"
        );
    }

    #[test]
    fn generate_default_and_named() {
        let imports = vec![ParsedImport {
            specifier: "pkg@1.0.0/+esm".into(),
            bindings: vec![
                ImportBinding::Default("D".into()),
                ImportBinding::Named {
                    imported: "a".into(),
                    local: "a".into(),
                },
            ],
        }];
        let code = generate_dynamic_imports(&imports);
        assert_eq!(
            code,
            "const { default: D, a } = await import(\"pkg@1.0.0/+esm\");\n"
        );
    }

    // -----------------------------------------------------------------------
    // Comment / string / template literal awareness
    // -----------------------------------------------------------------------

    #[test]
    fn import_in_block_comment_not_rewritten() {
        let code = "/*\nimport { z } from \"zod@3.22.4/+esm\";\n*/\nasync function main() {}";
        let result = rewrite_imports(code);
        assert!(result.imports.is_empty());
        assert_eq!(result.code, code);
    }

    #[test]
    fn import_in_single_line_comment_not_rewritten() {
        let code = "// import { z } from \"zod@3.22.4/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert!(result.imports.is_empty());
        assert_eq!(result.code, code);
    }

    #[test]
    fn import_in_template_literal_not_rewritten() {
        let code =
            "const s = `\nimport { z } from \"zod@3.22.4/+esm\";\n`;\nasync function main() {}";
        let result = rewrite_imports(code);
        assert!(result.imports.is_empty());
        assert_eq!(result.code, code);
    }

    #[test]
    fn import_in_double_quoted_string_not_rewritten() {
        let code = "const s = \"import { z } from 'zod'\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert!(result.imports.is_empty());
        assert_eq!(result.code, code);
    }

    #[test]
    fn real_import_after_block_comment() {
        let code =
            "/* comment */\nimport { z } from \"zod@3.22.4/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm");
    }

    #[test]
    fn async_function_main_in_comment_not_matched() {
        let code = "// async function main\nimport { z } from \"zod@3.22.4/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm");
    }

    #[test]
    fn async_function_main_in_string_not_matched() {
        let code = "const s = \"async function main\";\nimport { z } from \"zod@3.22.4/+esm\";\nasync function main() {}";
        let result = rewrite_imports(code);
        assert_eq!(result.imports.len(), 1);
        assert_eq!(result.imports[0].specifier, "zod@3.22.4/+esm");
    }
}
