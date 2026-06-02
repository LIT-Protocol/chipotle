import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Address } from 'viem';

/**
 * `local_test.sh` step 3 deploys the contracts to Anvil and writes the
 * AccountConfig diamond proxy address to `lit-api-server/NodeConfig.toml` in
 * this shape:
 *
 *   [chain]
 *   name = "anvil"
 *   contract_address = "0x…"
 *
 * Tests that need the proxy address read it from here instead of redeploying.
 */

const NODE_CONFIG_PATH =
  process.env.NODE_CONFIG_PATH ??
  resolve(process.cwd(), '..', 'lit-api-server', 'NodeConfig.toml');

export interface ChainConfig {
  name: string;
  contractAddress: Address;
}

let cache: ChainConfig | undefined;

export function loadChainConfig(): ChainConfig {
  if (cache) return cache;
  let raw: string;
  try {
    raw = readFileSync(NODE_CONFIG_PATH, 'utf8');
  } catch (err) {
    throw new Error(
      `Could not read ${NODE_CONFIG_PATH}. ` +
        `Did you start the local environment (./local_test.sh)? (${(err as Error).message})`,
    );
  }
  const name = matchLine(raw, /^\s*name\s*=\s*"([^"]+)"/m);
  const contractAddress = matchLine(raw, /^\s*contract_address\s*=\s*"(0x[0-9a-fA-F]{40})"/m);
  if (!name || !contractAddress) {
    throw new Error(
      `NodeConfig.toml at ${NODE_CONFIG_PATH} is missing chain.name or chain.contract_address.`,
    );
  }
  cache = { name, contractAddress: contractAddress as Address };
  return cache;
}

export function accountConfigAddress(): Address {
  return loadChainConfig().contractAddress;
}

function matchLine(text: string, re: RegExp): string | undefined {
  const m = re.exec(text);
  return m ? m[1] : undefined;
}
