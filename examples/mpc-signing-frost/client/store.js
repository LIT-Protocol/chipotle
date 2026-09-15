// Tiny JSON store for the user-held MPC material, written next to the example.
//
// `.mpc-store.json` (the "hot" store) holds the user's hot signing share
// (party 0), the action's signing share sealed to its CID (relayed back into
// the action at sign time), and the group public key / Solana address. This is
// the user's half of the key: if it is lost the key can't be used for normal
// signing (and Lit alone still can't sign).
//
// `.mpc-cold-share.json` (2-of-3 only) holds the cold recovery share (party 2).
// The user is meant to move it OFFLINE. It is never needed for normal signing;
// it exists so that hot + cold can sign without Lit if Lit ever disappears.
//
// Both files are gitignored — never commit them.

const fs = require("fs");
const path = require("path");

const STORE_PATH = path.join(__dirname, "..", ".mpc-store.json");
const COLD_PATH = path.join(__dirname, "..", ".mpc-cold-share.json");

function exists() {
  return fs.existsSync(STORE_PATH);
}

function load() {
  if (!exists()) {
    throw new Error("No MPC keyshare found. Run `npm run keygen` first.");
  }
  return JSON.parse(fs.readFileSync(STORE_PATH, "utf8"));
}

function save(data) {
  // 0600: this holds the user's hot signing share — keep it owner-only. chmod
  // after write enforces it even if the file already existed with looser perms.
  fs.writeFileSync(STORE_PATH, JSON.stringify(data, null, 2) + "\n", { mode: 0o600 });
  fs.chmodSync(STORE_PATH, 0o600);
  return STORE_PATH;
}

function coldExists() {
  return fs.existsSync(COLD_PATH);
}

function loadCold() {
  if (!coldExists()) {
    throw new Error(
      `No cold share at ${COLD_PATH}. Recovery signing needs the cold share — ` +
        "restore it from wherever you backed it up (the default `keygen` writes it; `--basic` does not)."
    );
  }
  return JSON.parse(fs.readFileSync(COLD_PATH, "utf8"));
}

function saveCold(data) {
  // 0600: the cold recovery share is half a recovery quorum — owner-only.
  fs.writeFileSync(COLD_PATH, JSON.stringify(data, null, 2) + "\n", { mode: 0o600 });
  fs.chmodSync(COLD_PATH, 0o600);
  return COLD_PATH;
}

module.exports = { exists, load, save, STORE_PATH, coldExists, loadCold, saveCold, COLD_PATH };
