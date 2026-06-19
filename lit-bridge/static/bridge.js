// lit-bridge UI — wallet-direct, no backend beyond /api/config.
// Burn on the source chain (attaching the relayer's native gas prepay); the
// relayer auto-mints on the destination. We poll usedBurnIds on-chain to show
// completion. Mirrors scripts/burn.js + watchRuns.js, client-side.

const MINT_GAS_LIMIT = 300000; // must match the action's MINT_GAS_LIMIT
const PREPAY_BUFFER = 2; // absorb gas drift between burn and mint
const TOKEN_ABI = [
  "function burn(uint256 amount, uint256 destChainId, address recipient) payable returns (uint256)",
  "function balanceOf(address) view returns (uint256)",
  "function decimals() view returns (uint8)",
  "function usedBurnIds(bytes32) view returns (bool)",
  "event BurnInitiated(address indexed from, address indexed recipient, uint256 amount, uint256 indexed destChainId, uint256 nonce, uint256 gasPrepaid)",
];
const BURN_TOPIC = ethers.utils.id("BurnInitiated(address,address,uint256,uint256,uint256,uint256)");

let cfg, account;
const $ = (id) => document.getElementById(id);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const chainById = (id) => cfg.chains.find((c) => c.chain_id === Number(id));
const rpc = (c) => new ethers.providers.JsonRpcProvider(c.rpc);

function setStatus(html, cls = "") {
  $("status").innerHTML = cls ? `<span class="${cls}">${html}</span>` : html;
}
function explorerTx(c, hash) {
  return c.explorer ? `<a href="${c.explorer.replace(/\/$/, "")}/tx/${hash}" target="_blank" rel="noreferrer">${hash.slice(0, 10)}…</a>` : hash.slice(0, 10) + "…";
}

async function init() {
  cfg = await (await fetch("/api/config")).json();
  if (!cfg.chains || cfg.chains.length < 2) {
    $("notconfigured").classList.remove("hidden");
    return;
  }
  $("bridge").classList.remove("hidden");
  const opts = cfg.chains.map((c) => `<option value="${c.chain_id}">${c.name}</option>`).join("");
  $("from").innerHTML = opts;
  $("to").innerHTML = opts;
  $("to").selectedIndex = 1;
  const sym = cfg.token_symbol || "tokens";
  $("feenote").textContent = `Fee: ${(cfg.fee_bps / 100).toFixed(2)}% · token: ${sym}`;

  $("connect").onclick = connect;
  $("go").onclick = bridge;
  $("swap").onclick = () => { const f = $("from").value; $("from").value = $("to").value; $("to").value = f; onChange(); };
  $("from").onchange = onChange;
  $("to").onchange = onChange;
  $("amount").oninput = onChange;
  if (window.ethereum) {
    window.ethereum.on?.("accountsChanged", (a) => { account = a[0]; afterConnect(); });
  }
}

async function connect() {
  if (!window.ethereum) { setStatus("No EVM wallet found (install MetaMask).", "err"); return; }
  try {
    [account] = await window.ethereum.request({ method: "eth_requestAccounts" });
    afterConnect();
  } catch (e) { setStatus(e.message, "err"); }
}

function afterConnect() {
  $("connect").textContent = account ? `${account.slice(0, 6)}…${account.slice(-4)}` : "Connect wallet";
  if (account && !$("recipient").value) $("recipient").value = account;
  onChange();
}

async function onChange() {
  const from = chainById($("from").value);
  $("go").disabled = !account;
  $("go").textContent = account ? "Bridge" : "Connect a wallet to bridge";
  if (!account) return;
  // balance on the source chain
  try {
    const token = new ethers.Contract(from.token, TOKEN_ABI, rpc(from));
    const [bal, dec] = await Promise.all([token.balanceOf(account), token.decimals()]);
    $("balance").textContent = `balance: ${(+ethers.utils.formatUnits(bal, dec)).toLocaleString()} ${cfg.token_symbol || ""}`;
  } catch { $("balance").textContent = ""; }
  // fee quote
  const amt = parseFloat($("amount").value);
  if (amt > 0) {
    const fee = amt * (cfg.fee_bps / 10000);
    $("quote").textContent = `recipient receives ~${(amt - fee).toLocaleString()} ${cfg.token_symbol || ""} (after ${(cfg.fee_bps / 100).toFixed(2)}% fee), plus a small native gas prepay you attach.`;
  } else $("quote").textContent = "";
}

async function ensureChain(chainId) {
  const hex = "0x" + Number(chainId).toString(16);
  try {
    await window.ethereum.request({ method: "wallet_switchEthereumChain", params: [{ chainId: hex }] });
  } catch (e) {
    if (e.code === 4902) {
      const c = chainById(chainId);
      await window.ethereum.request({
        method: "wallet_addEthereumChain",
        params: [{ chainId: hex, chainName: c.name, rpcUrls: [c.rpc], nativeCurrency: { name: "ETH", symbol: "ETH", decimals: 18 }, blockExplorerUrls: c.explorer ? [c.explorer] : [] }],
      });
    } else throw e;
  }
}

async function bridge() {
  const from = chainById($("from").value), to = chainById($("to").value);
  if (from.chain_id === to.chain_id) { setStatus("Pick two different chains.", "err"); return; }
  const recipient = ($("recipient").value || account).trim();
  if (!ethers.utils.isAddress(recipient)) { setStatus("Invalid recipient address.", "err"); return; }
  const amtStr = $("amount").value.trim();
  if (!(parseFloat(amtStr) > 0)) { setStatus("Enter an amount.", "err"); return; }

  $("go").disabled = true;
  try {
    setStatus(`Switching wallet to ${from.name}…`);
    await ensureChain(from.chain_id);
    const web3 = new ethers.providers.Web3Provider(window.ethereum);
    const token = new ethers.Contract(from.token, TOKEN_ABI, web3.getSigner());
    const dec = await token.decimals();
    const amount = ethers.utils.parseUnits(amtStr, dec);

    // Relayer gas prepay = dest gas price × MINT_GAS_LIMIT × buffer (same native).
    const gasPrice = await rpc(to).getGasPrice();
    const prepay = gasPrice.mul(MINT_GAS_LIMIT).mul(PREPAY_BUFFER);

    setStatus(`Burning ${amtStr} on ${from.name} (gas prepay ${(+ethers.utils.formatEther(prepay)).toFixed(6)} ETH)… confirm in your wallet.`);
    const tx = await token.burn(amount, to.chain_id, recipient, { value: prepay });
    setStatus(`Burn sent ${explorerTx(from, tx.hash)} — waiting for confirmation…`);
    const rcpt = await tx.wait();

    const log = rcpt.logs.find((l) => l.address.toLowerCase() === from.token.toLowerCase() && (l.topics[0] || "").toLowerCase() === BURN_TOPIC.toLowerCase());
    if (!log) throw new Error("BurnInitiated event not found");
    const burnId = ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(["uint256", "bytes32", "uint256"], [from.chain_id, tx.hash, log.logIndex]));

    setStatus(`Burned on ${from.name} ✓ — relaying to ${to.name} automatically (usually under a minute)…`);
    const destToken = new ethers.Contract(to.token, TOKEN_ABI, rpc(to));
    for (let i = 0; i < 120; i++) {
      try { if (await destToken.usedBurnIds(burnId)) { setStatus(`✓ Done — minted on ${to.name} for ${recipient.slice(0, 6)}…${recipient.slice(-4)}.`, "ok"); return; } } catch {}
      await sleep(5000);
    }
    setStatus(`Burn confirmed, but the mint hasn't landed yet. The relayer + retry poller will complete it; this will update once it does.\nburnId ${burnId}`);
  } catch (e) {
    setStatus(e.data?.message || e.reason || e.message || String(e), "err");
  } finally {
    $("go").disabled = false;
  }
}

init();
