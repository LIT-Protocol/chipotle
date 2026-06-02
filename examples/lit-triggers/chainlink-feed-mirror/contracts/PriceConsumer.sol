// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title PriceConsumer
/// @notice A mirrored price feed for a chain Chainlink does not natively serve.
/// The chain-feed-mirror Lit Action reads a Chainlink `AnswerUpdated` event on a
/// supported source chain and relays it here by calling `setPrice`. Only the
/// pinned `updater` — the wallet derived from the action's IPFS CID, a key no
/// human holds — can write, and only strictly-newer rounds are accepted.
contract PriceConsumer {
    address public immutable updater;

    int256 public answer;
    uint256 public roundId;
    uint256 public updatedAt;
    bool public initialized;

    event PriceUpdated(int256 answer, uint256 roundId, uint256 updatedAt);

    error NotUpdater();
    error StaleRound();

    constructor(address _updater) {
        updater = _updater;
    }

    function setPrice(int256 _answer, uint256 _roundId, uint256 _updatedAt) external {
        if (msg.sender != updater) revert NotUpdater();
        // Accept only newer rounds (Chainlink roundIds are monotonic). Use an
        // explicit `initialized` flag rather than `roundId != 0` as the
        // first-write sentinel, so a legitimate roundId of 0 can't repeatedly
        // bypass the stale-round check.
        if (initialized && _roundId <= roundId) revert StaleRound();
        initialized = true;
        answer = _answer;
        roundId = _roundId;
        updatedAt = _updatedAt;
        emit PriceUpdated(_answer, _roundId, _updatedAt);
    }

    function latestAnswer() external view returns (int256) {
        return answer;
    }
}
