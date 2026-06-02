// Minimal .env reader / upserter shared across all the scripts.
//
// We deliberately don't depend on `dotenv` so the helper scripts can run
// without `npm install` if someone is poking around. The semantics are:
//   - load(): parse .env into process.env (only setting keys that aren't
//     already set, so explicit env-var overrides on the command line win).
//   - upsert(key, value): if .env contains a line `KEY=...`, replace it;
//     otherwise append `KEY=value` to the end. Writes back via writeFileSync (not atomic; fine for a single-process script).
//
// Why upsert? The setup script generates derived values (PKP address,
// group ID, deployed contract address, ciphertexts) and stores them in
// .env so each step can record what it derived. Setup overwrites all
// of these on every run — see scripts/setup.js for the "fresh setup
// each time" rationale.

const fs = require("fs");
const path = require("path");

const ENV_PATH = path.join(__dirname, "..", ".env");

function load() {
  if (!fs.existsSync(ENV_PATH)) return;
  for (const line of fs.readFileSync(ENV_PATH, "utf8").split("\n")) {
    const m = line.match(/^\s*([A-Z_][A-Z0-9_]*)\s*=\s*(.*?)\s*$/);
    if (!m) continue;
    // Don't clobber values already in process.env — that lets callers
    // override on the command line (e.g. `FOO=bar node script.js`).
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
  fs.writeFileSync(ENV_PATH, next);
  process.env[key] = value;
}

module.exports = { load, upsert };
