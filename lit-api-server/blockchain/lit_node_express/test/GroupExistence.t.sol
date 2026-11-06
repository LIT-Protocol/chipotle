// SPDX-License-Identifier: MIT
pragma solidity =0.8.28;

import {BaseTest} from "./helpers/BaseTest.sol";
import {AppStorage} from "../contracts/AccountConfigFacets/AppStorage.sol";
import {ViewsFacet} from "../contracts/AccountConfigFacets/ViewsFacet.sol";

contract GroupExistenceTest is BaseTest {
    uint256 internal accountHash;
    address internal pkp = makeAddr("pkp");

    function setUp() public override {
        super.setUp();
        vm.prank(user);
        writes.newChainSecuredAccount("alice", "primary");
        accountHash = apiKeyHashOf(user);
    }

    function test_addPkpToGroup_zeroReverts() public {
        vm.prank(user);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.GroupDoesNotExist.selector, accountHash, 0)
        );
        writes.addPkpToGroup(accountHash, 0, pkp);
        assertEq(views_.listGroups(accountHash, 0, 10).length, 0);
        assertEq(views_.listGroupContents(accountHash, 0).pkpId.length, 0);
    }

    function testFuzz_addPkpToGroup_nonexistentReverts(uint256 groupId) public {
        vm.assume(groupId != 0);
        vm.prank(user);
        vm.expectRevert(
            abi.encodeWithSelector(AppStorage.GroupDoesNotExist.selector, accountHash, groupId)
        );
        writes.addPkpToGroup(accountHash, groupId, pkp);
    }

    function test_addPkpToGroup_createdGroupSucceeds() public {
        vm.prank(user);
        uint256 groupId =
            writes.addGroup(accountHash, "group", "", new uint256[](0), new address[](0));
        assertEq(groupId, 1);

        vm.prank(user);
        writes.addPkpToGroup(accountHash, groupId, pkp);

        ViewsFacet.GroupReturn memory group = views_.listGroupContents(accountHash, groupId);
        assertEq(group.pkpId.length, 1);
        assertEq(group.pkpId[0], pkp);
    }
}
