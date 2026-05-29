// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title ReleaseRegistry
/// @notice Tamper-evident, publicly verifiable record of canonical releases.
/// The webhook-release-attestation Lit Action calls `attest` after verifying a
/// GitHub release webhook. The caller (msg.sender) is the action's keyless
/// wallet — a key no human or server holds.
contract ReleaseRegistry {
    event Attested(
        address indexed attester,
        string repo,
        string tag,
        string commitish,
        uint256 timestamp
    );

    struct Release {
        string commitish;
        address attester;
        uint256 timestamp;
    }

    // repo => tag => Release
    mapping(string => mapping(string => Release)) private releases;

    function attest(string calldata repo, string calldata tag, string calldata commitish) external {
        releases[repo][tag] = Release({
            commitish: commitish,
            attester: msg.sender,
            timestamp: block.timestamp
        });
        emit Attested(msg.sender, repo, tag, commitish, block.timestamp);
    }

    function getRelease(string calldata repo, string calldata tag)
        external
        view
        returns (string memory commitish, address attester, uint256 timestamp)
    {
        Release storage r = releases[repo][tag];
        return (r.commitish, r.attester, r.timestamp);
    }
}
