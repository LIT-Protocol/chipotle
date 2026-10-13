// Produce the deployable bridge action by injecting the deploy-time trust-root
// constants (REGISTRY_ADDRESS, BRIDGE_PKP_ID) into the source. These ARE part
// of the CID by design — REGISTRY_ADDRESS is the trust root and changing the
// PKP changes the CID. The built file is environment-specific (gitignored); the
// committed bridgeAction.js keeps placeholders so its pure helpers stay unit-
// testable.

const fs = require("fs");
const path = require("path");

const SRC = path.join(__dirname, "..", "action", "bridgeAction.js");
const OUT = path.join(__dirname, "..", "action", "bridgeAction.built.js");

function buildAction(registryAddress, pkpId) {
  let code = fs.readFileSync(SRC, "utf8");

  const before = code;
  code = code.replace(
    /const REGISTRY_ADDRESS = "0x0000000000000000000000000000000000000000";.*$/m,
    `const REGISTRY_ADDRESS = "${registryAddress}";`
  );
  code = code.replace(
    /const BRIDGE_PKP_ID = "REPLACE_WITH_PKP_ID";/,
    `const BRIDGE_PKP_ID = "${pkpId}";`
  );

  if (code === before || code.includes("REPLACE_WITH_PKP_ID")) {
    throw new Error(
      "buildAction: failed to inject constants — placeholders not found in bridgeAction.js"
    );
  }
  if (!code.includes(`"${registryAddress}"`) || !code.includes(`"${pkpId}"`)) {
    throw new Error("buildAction: injected constants missing after replace");
  }

  fs.writeFileSync(OUT, code);
  return code;
}

module.exports = { buildAction, BUILT_PATH: OUT };
