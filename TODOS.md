# TODOS

## Monitor: Keyboard Shortcuts
- **What:** Add keyboard shortcuts to the Lit Node Monitor: R to refresh, F to fund all critical, S to toggle settings panel.
- **Why:** Operators use this tool daily. Keyboard shortcuts reduce friction for the most common actions.
- **Effort:** S (CC: ~5 min)
- **Priority:** P3
- **Depends on:** Phase 1 and Phase 2 payer safety console features
- **Context:** Deferred during CEO review of the payer safety console plan. Avoid conflicts with browser shortcuts (Ctrl+R, etc.) — use single-key shortcuts only when no input field is focused.

## Monitor: Network Health Badge in Dropdown
- **What:** Show a colored dot (green/yellow/red) next to each network name in the network selector dropdown, based on aggregate payer pool health for that network.
- **Why:** Operators currently must switch to each network to check payer health. A badge gives cross-network awareness at a glance.
- **Effort:** M (CC: ~15 min)
- **Priority:** P2
- **Depends on:** Phase 1 health state logic
- **Context:** Deferred during CEO review. Main complexity: requires background polling of all networks' payer balances simultaneously (not just the selected network), which increases RPC calls. Consider polling non-selected networks at a lower frequency (e.g., every 2 minutes vs 30 seconds for the active network).
