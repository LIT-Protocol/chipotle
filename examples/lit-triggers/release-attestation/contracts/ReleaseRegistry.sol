// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title ReleaseRegistry
/// @notice Tamper-evident, publicly verifiable record of canonical releases.
/// Only the `attester` — the wallet derived from the release-attestation Lit
/// Action's IPFS CID, a key no human holds — can write. The action verifies a
/// GitHub release webhook (HMAC) off-chain, then sends `attest` from that
/// wallet. Edit the action and its CID/signer change, so a modified action can
/// no longer write here.
contract ReleaseRegistry {
    address public immutable attester;

    event Attested(
        address indexed attester,
        string repo,
        string tag,
        string commitish,
        uint256 timestamp
    );

    struct Release {
        string commitish;
        uint256 timestamp;
    }

    // repo => tag => Release
    mapping(string => mapping(string => Release)) private releases;

    error NotAttester();

    constructor(address _attester) {
        attester = _attester;
    }

    function attest(string calldata repo, string calldata tag, string calldata commitish) external {
        if (msg.sender != attester) revert NotAttester();
        releases[repo][tag] = Release({commitish: commitish, timestamp: block.timestamp});
        emit Attested(msg.sender, repo, tag, commitish, block.timestamp);
    }

    function getRelease(string calldata repo, string calldata tag)
        external
        view
        returns (string memory commitish, uint256 timestamp)
    {
        Release storage r = releases[repo][tag];
        return (r.commitish, r.timestamp);
    }
}
