// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {BridgeToken} from "../src/BridgeToken.sol";

contract BridgeTokenTest is Test {
    BridgeToken token;

    uint256 oraclePk;
    address oracle;
    address treasury = address(0x7EE);
    address recipient = address(0xBEEF);
    address stranger = address(0xBAD);

    // A fixed burn the destination is redeeming.
    uint256 srcChainId = 8453;
    address srcContract = address(0x5012);
    bytes32 burnTxHash = keccak256("burn");
    uint256 logIndex = 3;
    uint256 srcNonce = 1;

    function setUp() public {
        oraclePk = 0xA11CE;
        oracle = vm.addr(oraclePk);
        token = new BridgeToken("Bridge Coin", "BRDG", 1000 ether, oracle); // this == owner, holds supply
        token.setBridgePartner(srcChainId, srcContract);
    }

    function _sign(address to, uint256 amount, uint256 deadline) internal view returns (bytes memory) {
        bytes32 inner = keccak256(
            abi.encode(
                srcChainId, srcContract, burnTxHash, logIndex, to,
                amount, srcNonce, deadline, address(token), block.chainid
            )
        );
        bytes32 ethHash = MessageHashUtils.toEthSignedMessageHash(inner);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(oraclePk, ethHash);
        return abi.encodePacked(r, s, v);
    }

    function _mint(uint256 amount) internal {
        uint256 deadline = block.timestamp + 3600;
        bytes memory sig = _sign(recipient, amount, deadline);
        token.mint(srcChainId, srcContract, burnTxHash, logIndex, recipient, amount, srcNonce, deadline, sig);
    }

    function test_mintNoFeeWhenTreasuryUnset() public {
        _mint(1000 ether);
        assertEq(token.balanceOf(recipient), 1000 ether);
        assertEq(token.balanceOf(treasury), 0);
    }

    function test_mintWithBpsFee() public {
        token.setFeeConfig(treasury, 0, 100); // 1%
        _mint(1000 ether);
        assertEq(token.balanceOf(treasury), 10 ether);
        assertEq(token.balanceOf(recipient), 990 ether);
    }

    function test_mintWithFlatPlusBpsFee() public {
        token.setFeeConfig(treasury, 1 ether, 100); // 1 flat + 1%
        _mint(1000 ether);
        assertEq(token.balanceOf(treasury), 11 ether);
        assertEq(token.balanceOf(recipient), 989 ether);
    }

    function test_feeClampNeverRevertsOnDust() public {
        // Flat fee larger than the transfer: clamp to amount, recipient gets 0,
        // and crucially the mint does NOT revert (source tokens already burned).
        token.setFeeConfig(treasury, 5 ether, 0);
        _mint(2 ether);
        assertEq(token.balanceOf(treasury), 2 ether);
        assertEq(token.balanceOf(recipient), 0);
    }

    function test_replayRejected() public {
        _mint(100 ether);
        uint256 deadline = block.timestamp + 3600;
        bytes memory sig = _sign(recipient, 100 ether, deadline);
        vm.expectRevert(BridgeToken.AlreadyRedeemed.selector);
        token.mint(srcChainId, srcContract, burnTxHash, logIndex, recipient, 100 ether, srcNonce, deadline, sig);
    }

    function test_sourceContractMismatch() public {
        uint256 deadline = block.timestamp + 3600;
        bytes memory sig = _sign(recipient, 100 ether, deadline);
        vm.expectRevert(BridgeToken.SourceContractMismatch.selector);
        token.mint(srcChainId, address(0xDEAD), burnTxHash, logIndex, recipient, 100 ether, srcNonce, deadline, sig);
    }

    function test_badSignatureRejected() public {
        uint256 deadline = block.timestamp + 3600;
        // Sign with a different key than the oracle.
        bytes32 inner = keccak256(
            abi.encode(srcChainId, srcContract, burnTxHash, logIndex, recipient, 100 ether, srcNonce, deadline, address(token), block.chainid)
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(0xBEEF, MessageHashUtils.toEthSignedMessageHash(inner));
        vm.expectRevert(BridgeToken.InvalidBridgeSignature.selector);
        token.mint(srcChainId, srcContract, burnTxHash, logIndex, recipient, 100 ether, srcNonce, deadline, abi.encodePacked(r, s, v));
    }

    function test_twoStepOwnershipTransferToSafe() public {
        address safe = address(0x5AFE);
        token.transferOwnership(safe); // current owner (this) proposes
        assertEq(token.owner(), address(this)); // not yet — two-step
        assertEq(token.pendingOwner(), safe);
        vm.prank(safe);
        token.acceptOwnership();
        assertEq(token.owner(), safe);
        // old owner can no longer wire/config
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, address(this)));
        token.setFeeConfig(treasury, 0, 50);
    }

    function test_burnEscrowsGasPrepay() public {
        uint256 prepay = 0.001 ether;
        vm.deal(address(this), prepay);
        token.burn{value: prepay}(100 ether, srcChainId, recipient);
        assertEq(address(token).balance, prepay); // prepay pooled in the contract
    }

    function test_sweepGasOwnerOnly() public {
        vm.deal(address(this), 0.002 ether);
        token.burn{value: 0.002 ether}(50 ether, srcChainId, recipient);

        vm.prank(stranger);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, stranger));
        token.sweepGas(stranger);

        address sink = address(0x5151);
        token.sweepGas(sink);
        assertEq(sink.balance, 0.002 ether);
        assertEq(address(token).balance, 0);
    }

    function test_setFeeConfigOnlyOwnerAndCap() public {
        vm.prank(stranger);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, stranger));
        token.setFeeConfig(treasury, 0, 100);

        vm.expectRevert(BridgeToken.FeeBpsTooHigh.selector);
        token.setFeeConfig(treasury, 0, 501); // > MAX_FEE_BPS (500)

        token.setFeeConfig(treasury, 0, 500); // exactly the cap is fine
        assertEq(token.feeBps(), 500);
    }
}
