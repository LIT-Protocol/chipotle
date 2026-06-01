// Minimal .env reader / upserter shared across the release-attestation scripts.
//
// Kept inline so this example folder is self-contained and you can clone it
// without copying siblings.

const fs = require("fs");
const path = require("path");

const ENV_PATH = path.join(__dirname, "..", ".env");

function load() {
  if (!fs.existsSync(ENV_PATH)) return;
  for (const line of fs.readFileSync(ENV_PATH, "utf8").split("\n")) {
    const m = line.match(/^\s*([A-Z_][A-Z0-9_]*)\s*=\s*(.*?)\s*$/);
    if (!m) continue;
    // Command-line / pre-existing env wins so callers can override.
    if (!process.env[m[1]]) {
      process.env[m[1]] = m[2].replace(/^["']|["']$/g, "");
    }
  }
}

function upsert(key, value) {
  const text = fs.existsSync(ENV_PATH) ? fs.readFileSync(ENV_PATH, "utf8") : "";
  const line = `${key}=${value}`;
  const re = new RegExp(`^${key}\\s*=.*$`, "m");
  let next;
  if (re.test(text)) {
    next = text.replace(re, line);
  } else {
    next =
      text.endsWith("\n") || text.length === 0
        ? text + line + "\n"
        : text + "\n" + line + "\n";
  }
  fs.writeFileSync(ENV_PATH, next);
  process.env[key] = value;
}

module.exports = { load, upsert };
