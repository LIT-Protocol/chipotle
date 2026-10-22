// lit-bridge UI — wallet-direct, no backend beyond /api/config.
// Burn on the source chain (attaching the relayer's native gas prepay); the
// relayer auto-mints on the destination. We poll usedBurnIds on-chain to show
// completion. Mirrors scripts/burn.js + watchRuns.js, client-side.
//
// Trust note: config values (chain names, explorer URLs) and external strings
// (RPC/wallet error messages) are rendered as TEXT, never HTML — no innerHTML on
// any value we don't fully control. See setStatus/setText below.

const MINT_GAS_LIMIT = 300000; // must match the action's MINT_GAS_LIMIT
const PREPAY_BUFFER = 2; // attach 2x the action's 1x requirement to absorb gas drift
const PREPAY_WARN_ETH = 0.02; // soft sanity ceiling: warn if a quote exceeds this
const TOKEN_ABI = [
  "function burn(uint256 amount, uint256 destChainId, address recipient) payable returns (uint256)",
  "function balanceOf(address) view returns (uint256)",
  "function decimals() view returns (uint8)",
  "function usedBurnIds(bytes32) view returns (bool)",
  "event BurnInitiated(address indexed from, address indexed recipient, uint256 amount, uint256 indexed destChainId, uint256 nonce, uint256 gasPrepaid)",
];
const BURN_TOPIC = ethers.utils.id("BurnInitiated(address,address,uint256,uint256,uint256,uint256)");

let cfg, account;
let recipientAutoFilled = false; // true while #recipient mirrors the connected account
let balanceSeq = 0; // guards against stale async balance writes
const $ = (id) => document.getElementById(id);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const chainById = (id) => cfg.chains.find((c) => c.chain_id === Number(id));
const rpc = (c) => new ethers.providers.JsonRpcProvider(c.rpc);
const sym = () => cfg.token_symbol || "";

// Only http(s) explorer URLs become links — never javascript:/data: etc. Anything
// else falls back to plain text. Returns an <a> or text node (never raw HTML).
function explorerTxNode(c, hash) {
  let url = null;
  try {
    const u = new URL(`${c.explorer.replace(/\/$/, "")}/tx/${hash}`);
    if (u.protocol === "http:" || u.protocol === "https:") url = u.href;
  } catch { /* no/invalid explorer → text only */ }
  if (!url) return document.createTextNode(hash.slice(0, 10) + "…");
  const a = document.createElement("a");
  a.href = url; a.target = "_blank"; a.rel = "noreferrer";
  a.textContent = hash.slice(0, 10) + "…";
  return a;
}

// Status renderer: accepts plain text or an array of (string | Node) parts. Never
// interprets HTML, so RPC/wallet/config strings can't inject markup.
function setStatus(parts, cls = "") {
  const el = $("status");
  el.replaceChildren();
  const span = document.createElement("span");
  if (cls) span.className = cls;
  for (const p of Array.isArray(parts) ? parts : [parts]) {
    span.appendChild(typeof p === "string" ? document.createTextNode(p) : p);
  }
  el.appendChild(span);
}

async function init() {
  cfg = await (await fetch("/api/config")).json();
  if (!cfg.chains || cfg.chains.length < 2) {
    $("notconfigured").classList.remove("hidden");
    return;
  }
  $("bridge").classList.remove("hidden");
  for (const sel of [$("from"), $("to")]) {
    sel.replaceChildren();
    for (const c of cfg.chains) {
      const o = document.createElement("option");
      o.value = String(c.chain_id);
      o.textContent = c.name; // textContent, not innerHTML — name is config-controlled
      sel.appendChild(o);
    }
  }
  $("to").selectedIndex = 1;
  $("feenote").textContent = `Fee: ${feeLabel()} · token: ${cfg.token_symbol || "tokens"}`;

  $("connect").onclick = connect;
  $("go").onclick = bridge;
  $("swap").onclick = () => { const f = $("from").value; $("from").value = $("to").value; $("to").value = f; onChange(); };
  $("from").onchange = onChange;
  $("to").onchange = onChange;
  $("amount").oninput = onChange;
  $("recipient").oninput = () => { recipientAutoFilled = false; };
  if (window.ethereum) {
    window.ethereum.on?.("accountsChanged", (a) => { account = a[0]; afterConnect(); });
    window.ethereum.on?.("chainChanged", () => onChange());
  }
}

function feeLabel() {
  const flat = parseFloat(cfg.fee_flat || "0") || 0;
  const bps = `${(cfg.fee_bps / 100).toFixed(2)}%`;
  return flat > 0 ? `${flat} ${sym()} + ${bps}` : bps;
}

async function connect() {
  if (!window.ethereum) { setStatus("No EVM wallet found (install MetaMask).", "err"); return; }
  try {
    [account] = await window.ethereum.request({ method: "eth_requestAccounts" });
    afterConnect();
  } catch (e) { setStatus(errText(e), "err"); }
}

function afterConnect() {
  $("connect").textContent = account ? `${account.slice(0, 6)}…${account.slice(-4)}` : "Connect wallet";
  // Keep the recipient in sync with the connected account WHILE it's auto-filled,
  // so switching accounts doesn't silently mint to the previous one. Once the user
  // edits the field, we stop touching it.
  if (account && (recipientAutoFilled || !$("recipient").value)) {
    $("recipient").value = account;
    recipientAutoFilled = true;
  }
  onChange();
}

async function onChange() {
  const from = chainById($("from").value);
  $("go").disabled = !account;
  $("go").textContent = account ? "Bridge" : "Connect a wallet to bridge";
  if (!account || !from) { $("balance").textContent = ""; }
  // balance on the source chain (sequence-guarded so a slow stale read can't
  // overwrite a newer one after the user changes chain/account)
  if (account && from) {
    const seq = ++balanceSeq;
    try {
      const token = new ethers.Contract(from.token, TOKEN_ABI, rpc(from));
      const [bal, dec] = await Promise.all([token.balanceOf(account), token.decimals()]);
      if (seq === balanceSeq) $("balance").textContent = `balance: ${(+ethers.utils.formatUnits(bal, dec)).toLocaleString()} ${sym()}`;
    } catch { if (seq === balanceSeq) $("balance").textContent = ""; }
  }
  // fee quote (flat + bps, mirroring the contract)
  const amt = parseFloat($("amount").value);
  if (amt > 0) {
    const flat = parseFloat(cfg.fee_flat || "0") || 0;
    const fee = flat + amt * (cfg.fee_bps / 10000);
    const net = amt - fee;
    $("quote").textContent = net > 0
      ? `recipient receives ~${net.toLocaleString()} ${sym()} (after ${feeLabel()} fee), plus a native gas prepay you attach.`
      : `⚠ the ${feeLabel()} fee meets or exceeds this amount — the recipient would get ~0. Bridge more.`;
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

// Normalize wallet/RPC errors to a string (they're rendered as text, not HTML).
function errText(e) {
  return String(e?.data?.message || e?.reason || e?.message || e || "unknown error");
}

async function bridge() {
  const from = chainById($("from").value), to = chainById($("to").value);
  if (from.chain_id === to.chain_id) { setStatus("Pick two different chains.", "err"); return; }
  const recipient = ($("recipient").value || account || "").trim();
  if (!ethers.utils.isAddress(recipient)) { setStatus("Invalid recipient address.", "err"); return; }
  const amtStr = $("amount").value.trim();
  if (!(parseFloat(amtStr) > 0)) { setStatus("Enter an amount.", "err"); return; }

  $("go").disabled = true;
  try {
    setStatus(`Switching wallet to ${from.name}…`);
    await ensureChain(from.chain_id);
    const web3 = new ethers.providers.Web3Provider(window.ethereum);
    // Verify the wallet actually landed on the source chain before sending — a
    // switch race or a wallet quirk could otherwise send against the wrong chain.
    const net = await web3.getNetwork();
    if (net.chainId !== from.chain_id) {
      setStatus(`Your wallet is on chain ${net.chainId}, not ${from.name} (${from.chain_id}). Switch and retry.`, "err");
      return;
    }
    const token = new ethers.Contract(from.token, TOKEN_ABI, web3.getSigner());
    const dec = await token.decimals();
    const amount = ethers.utils.parseUnits(amtStr, dec);

    // Relayer gas prepay = dest gas price × MINT_GAS_LIMIT × buffer (same native).
    // The action requires ≥ gasPrice × MINT_GAS_LIMIT at relay time; the 2x buffer
    // absorbs drift. A stale/hostile dest RPC could report an absurd price, so warn
    // before the user signs an oversized value (it pools in the token, not refunded).
    const gasPrice = await rpc(to).getGasPrice();
    const prepay = gasPrice.mul(MINT_GAS_LIMIT).mul(PREPAY_BUFFER);
    const prepayEth = +ethers.utils.formatEther(prepay);
    if (prepayEth > PREPAY_WARN_ETH) {
      const ok = window.confirm(`The destination RPC quotes a gas prepay of ${prepayEth.toFixed(6)} ETH, which is unusually high. It is attached as msg.value and is NOT refunded if unused. Continue?`);
      if (!ok) { setStatus("Cancelled — gas prepay looked too high.", "err"); return; }
    }

    setStatus(`Burning ${amtStr} on ${from.name} (gas prepay ${prepayEth.toFixed(6)} ETH)… confirm in your wallet.`);
    const tx = await token.burn(amount, to.chain_id, recipient, { value: prepay });
    setStatus(["Burn sent ", explorerTxNode(from, tx.hash), " — waiting for confirmation…"]);
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
    // Still not minted after ~10 min. The burn is final; recovery is the retry
    // poller (if the prepay covers current gas) or self-submitting mint with the
    // oracle signature. Don't over-promise automatic completion.
    setStatus(
      `Burn confirmed on ${from.name}, but the mint hasn't landed within ~10 min. ` +
      `The retry poller will complete it once the attached prepay covers destination gas; ` +
      `if gas spiked past the prepay, the recipient can self-submit the mint and pay their own gas. ` +
      `Save this burnId: ${burnId}`,
      "err",
    );
  } catch (e) {
    setStatus(errText(e), "err");
  } finally {
    $("go").disabled = false;
  }
}

init();
