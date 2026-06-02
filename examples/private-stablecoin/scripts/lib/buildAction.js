// Bakes deployment-specific config into an action's source before it's
// uploaded / CID-computed. The ledger PKP address isn't known until setup
// mints it, but it must be bound into the action (not passed at runtime) so a
// caller can't redirect note encryption to a PKP they control. Because the
// CID is computed from this baked source — and the PrivUSD contract pins the
// CID-derived signer — the config is cryptographically tied to the deployment.
//
// Both setup.js (computes CID, derives signer, registers) and demo.js (sends
// the action to /lit_action) build identical source via this helper, so the
// CID they reference matches.

const fs = require("fs");
const path = require("path");

function buildAction(file, replacements) {
  let src = fs.readFileSync(path.join(__dirname, "..", "..", "action", file), "utf8");
  for (const [needle, value] of Object.entries(replacements)) {
    if (!src.includes(needle)) {
      throw new Error(`buildAction: placeholder ${needle} not found in ${file}`);
    }
    src = src.split(needle).join(value);
  }
  return src;
}

// Convenience: build both actions for a given ledger PKP address.
function buildActions(ledgerPkpAddress) {
  const repl = { "__LEDGER_PKP_ID__": ledgerPkpAddress };
  return {
    ledgerCode: buildAction("ledger.js", repl),
    discloseCode: buildAction("disclose.js", repl),
  };
}

module.exports = { buildAction, buildActions };
