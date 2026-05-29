// Minimal .env reader / upserter shared across the scripts.
//
// We deliberately don't depend on `dotenv` so the helper scripts can run
// without `npm install` if someone is poking around. Semantics:
//   - load(): parse .env into process.env (only keys not already set, so
//     command-line overrides like `FOO=bar node script.js` win).
//   - upsert(key, value): replace an existing `KEY=...` line, else append.
//
// Setup generates derived values (action CID, signer address, deployed
// contract addresses, usage key) and records them in .env so each step can
// see what the previous one produced.

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
  fs.writeFileSync(ENV_PATH, next);
  process.env[key] = value;
}

module.exports = { load, upsert };
