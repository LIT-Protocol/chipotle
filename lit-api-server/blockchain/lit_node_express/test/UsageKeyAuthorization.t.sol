// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;
import {BaseTest} from "./helpers/BaseTest.sol";
import {AppStorage} from "../contracts/AccountConfigFacets/AppStorage.sol";

contract UsageKeyAuthorizationTest is BaseTest {
    uint256 constant MASTER = 12345;
    uint256 constant USAGE = 67890;

    function setUp() public override {
        super.setUp();
        vm.prank(apiPayer);
        writes.newAccount(MASTER, true, "managed", "", user);
        vm.prank(apiPayer);
        setKey(MASTER, USAGE, false);
    }

    function setKey(uint256 account, uint256 target, bool elevated) internal {
        uint256[] memory empty = new uint256[](0);
        writes.setUsageApiKey(
            account,
            target,
            block.timestamp + 7 days,
            0,
            "key",
            "",
            elevated,
            elevated,
            elevated,
            empty,
            empty,
            empty,
            empty
        );
    }

    function test_scopedHashCannotRewriteSelfViaPayer() public {
        vm.prank(apiPayer);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.NotMasterAccount.selector, USAGE));
        setKey(USAGE, USAGE, true);
    }

    function test_scopedHashCannotMintViaPayer() public {
        vm.prank(apiPayer);
        vm.expectRevert(abi.encodeWithSelector(AppStorage.NotMasterAccount.selector, USAGE));
        setKey(USAGE, 99999, true);
    }

    function test_unprivilegedWalletCannotUseScopedHashDirectly() public {
        vm.prank(stranger);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.NoAccountAccess.selector, USAGE, stranger)
        );
        setKey(USAGE, USAGE, true);
    }

    function test_masterStillCreatesAndUpdates() public {
        vm.startPrank(apiPayer);
        setKey(MASTER, USAGE, true);
        setKey(MASTER, 99999, true);
        vm.stopPrank();
        assertEq(views_.listApiKeys(MASTER, 0, 10).length, 2);
    }
}
