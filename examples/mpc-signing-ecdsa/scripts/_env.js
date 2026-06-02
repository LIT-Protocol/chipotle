// Minimal .env reader / upserter shared across all the scripts.
//
// See the other example folders for the same helper — kept inline here
// so each example folder is fully self-contained.

const fs = require("fs");
const path = require("path");

const ENV_PATH = path.join(__dirname, "..", ".env");

function load() {
  if (!fs.existsSync(ENV_PATH)) return;
  for (const line of fs.readFileSync(ENV_PATH, "utf8").split("\n")) {
    const m = line.match(/^\s*([A-Z_][A-Z0-9_]*)\s*=\s*(.*?)\s*$/);
    if (!m) continue;
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
    next = text.endsWith("\n") || text.length === 0 ? text + line + "\n" : text + "\n" + line + "\n";
  }
  // 0600: .env holds the scoped usage API key and deployer/executor private
  // keys — keep it owner-only, and enforce it on pre-existing files too.
  fs.writeFileSync(ENV_PATH, next, { mode: 0o600 });
  fs.chmodSync(ENV_PATH, 0o600);
  process.env[key] = value;
}

module.exports = { load, upsert };
