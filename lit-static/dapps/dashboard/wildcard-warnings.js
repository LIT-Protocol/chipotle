const ZERO_ADDRESS = '0x0000000000000000000000000000000000000000';

// Read every page until the wildcard is found or the membership list ends.
// No negative cache: edits and reloads must observe current permissions.
export async function populateWildcardWarnings({
  rows, getClient, apiKey, pageSize, isCurrent, onWarning, onError,
}) {
  let client;
  try {
    client = await getClient();
  } catch (error) {
    if (isCurrent()) onError(error);
    return;
  }
  let next = 0;
  async function worker() {
    while (isCurrent() && next < rows.length) {
      const row = rows[next++];
      try {
        for (let pageNumber = 0; isCurrent(); pageNumber++) {
          const wallets = await client.listWalletsInGroup({
            apiKey, groupId: row.groupId,
            pageNumber: String(pageNumber), pageSize: String(pageSize),
          });
          // An old response must never update a newer render/account or
          // schedule another request after logout, reload, or mode changes.
          if (!isCurrent()) return;
          if (wallets.some((w) => (w.wallet_address || '').toLowerCase() === ZERO_ADDRESS)) {
            onWarning(row);
            break;
          }
          if (wallets.length < pageSize) break;
        }
      } catch (error) {
        if (isCurrent()) onError(error);
      }
    }
  }
  await Promise.all(Array.from({ length: Math.min(4, rows.length) }, worker));
}
