/**
 * Lit Admin – read AccountConfig contract view functions directly.
 * ABI subset matches lit-api-server/src/accounts/contracts/AccountConfig.json (view functions only).
 */

const ACCOUNT_CONFIG_VIEW_ABI = [
  {
    inputs: [],
    name: 'api_payer',
    outputs: [{ internalType: 'address', name: '', type: 'address' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'pricing_operator',
    outputs: [{ internalType: 'address', name: '', type: 'address' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'nextAccountCount',
    outputs: [{ internalType: 'uint256', name: '', type: 'uint256' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [],
    name: 'nextWalletCount',
    outputs: [{ internalType: 'uint256', name: '', type: 'uint256' }],
    stateMutability: 'view',
    type: 'function',
  },
  {
    inputs: [{ internalType: 'uint256', name: 'index', type: 'uint256' }],
    name: 'indexToAccountHashAt',
    outputs: [{ internalType: 'uint256', name: '', type: 'uint256' }],
    stateMutability: 'view',
    type: 'function',
  },
];

function el(id) {
  return document.getElementById(id);
}

function showError(msg) {
  const err = el('error');
  if (err) {
    err.textContent = msg;
    err.style.display = 'block';
  }
}

function hideError() {
  const err = el('error');
  if (err) err.style.display = 'none';
}

function setValue(id, text, isEmpty) {
  const node = el(id);
  if (!node) return;
  node.textContent = text ?? '—';
  node.classList.toggle('empty', isEmpty);
}

async function fetchContractValues() {
  const rpcUrl = (el('rpc-url')?.value || '').trim();
  const contractAddress = (el('contract-address')?.value || '').trim();
  const btn = el('btn-fetch');
  const results = el('results');

  if (!rpcUrl || !contractAddress) {
    showError('Enter RPC URL and contract address.');
    return;
  }

  hideError();
  if (btn) btn.disabled = true;
  setValue('val-api-payer', '…', false);
  setValue('val-pricing-operator', '…', false);
  setValue('val-next-account-count', '…', false);
  setValue('val-next-wallet-count', '…', false);
  setValue('val-index-to-account', '…', false);
  if (results) results.style.display = 'block';

  try {
    const provider = new ethers.JsonRpcProvider(rpcUrl);
    const contract = new ethers.Contract(contractAddress, ACCOUNT_CONFIG_VIEW_ABI, provider);

    const [apiPayer, pricingOperator, nextAccountCount, nextWalletCount, indexToAccountHashAt1] = await Promise.all([
      contract.api_payer(),
      contract.pricing_operator(),
      contract.nextAccountCount(),
      contract.nextWalletCount(),
      contract.indexToAccountHashAt(1),
    ]);

    setValue('val-api-payer', apiPayer ?? '—', !apiPayer);
    setValue('val-pricing-operator', pricingOperator ?? '—', !pricingOperator);
    setValue('val-next-account-count', nextAccountCount != null ? String(nextAccountCount) : '—', nextAccountCount == null);
    setValue('val-next-wallet-count', nextWalletCount != null ? String(nextWalletCount) : '—', nextWalletCount == null);
    setValue('val-index-to-account', indexToAccountHashAt1 != null ? String(indexToAccountHashAt1) : '—', indexToAccountHashAt1 == null);
  } catch (e) {
    showError(e?.message || String(e));
  } finally {
    if (btn) btn.disabled = false;
  }
}

el('btn-fetch')?.addEventListener('click', fetchContractValues);
