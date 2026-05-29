// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title PriceConsumer
/// @notice Minimal mirrored price feed for a chain Chainlink does not support.
/// The chain-feed-mirror Lit Action calls `setPrice` with values read from a
/// Chainlink AnswerUpdated event on a supported source chain. The updater
/// (msg.sender) is the action's keyless relayer wallet.
///
/// For production you would restrict `setPrice` to the known relayer address
/// and reject stale/older rounds; kept permissionless here for a simple demo.
contract PriceConsumer {
    event PriceUpdated(int256 answer, uint256 roundId, uint256 updatedAt, address updater);

    int256 public answer;
    uint256 public roundId;
    uint256 public updatedAt;
    address public lastUpdater;

    function setPrice(int256 _answer, uint256 _roundId, uint256 _updatedAt) external {
        // Only accept newer rounds (monotonic), matching Chainlink semantics.
        require(_roundId >= roundId, "stale round");
        answer = _answer;
        roundId = _roundId;
        updatedAt = _updatedAt;
        lastUpdater = msg.sender;
        emit PriceUpdated(_answer, _roundId, _updatedAt, msg.sender);
    }

    function latestAnswer() external view returns (int256) {
        return answer;
    }
}
