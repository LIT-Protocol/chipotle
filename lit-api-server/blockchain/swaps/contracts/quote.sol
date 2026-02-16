// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// import enumerableSet from openzeppelin
import { EnumerableSet } from "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";

// Because we're not storing funds, do we need to use ReentrancyGuard?
// import { ReentrancyGuard } from "@openzeppelin/contracts/security/ReentrancyGuard.sol";

/**
 * Storage for GetSwapRequest data (mirrors GetSwapRequest in swaps/models.rs).
 * Optional fields use empty string or 0 when not set.
 */
contract QuoteStorage {
    
    using EnumerableSet for EnumerableSet.AddressSet;
    using EnumerableSet for EnumerableSet.UintSet;

    enum QuotePricingType {
        Origin,      // Destination amount fixed; origin amount increases
        Destination  // Origin fixed; destination amount reduced
    }

    struct SwapRequest {
        address from;                      // Message sender (origin of the swap)
        string originSymbol;
        string originChain;
        uint256 originAmount;
        string destinationSymbol;
        string destinationChain;
        uint256 destinationAmount;
        uint256 slippage;                  // Acceptable pricing slippage
        QuotePricingType pricingType;     // Where fees are taken from
        uint256 quoteDeadlineSeconds;       // 0–60 seconds
        address originAddress;             
        address refundAddress;             
        uint256 transactionDeadlineSeconds; // Optional; 0 if not set
        address pkpAddress;  // technically this could go into the quote
        uint256 pkpTokenId;
        string message;                   
    }

    struct Quote {        
        uint256 swapRequestId;
        address providerRefundAddress;
        uint256 quoteExpiry;
        uint256 createdAt;
        uint256 feesTotal;        
    }

    EnumerableSet.UintSet openQuotes;
    EnumerableSet.UintSet openSwapRequests;
    mapping(uint256 => Quote) quotes;
    mapping(uint256 => SwapRequest) swapRequests;
    mapping(address => Quote) quoteByPkpAddress;
    mapping(address => SwapRequest) lastSwapRequestFromAddress;

    uint256 public swapRequestCounter;
    uint256 public quoteCounter;

    constructor() {
        swapRequestCounter = 0;
        quoteCounter = 0;
    }

    function newSwapRequest(SwapRequest calldata data) external returns (uint256) {
        swapRequestCounter++;
        swapRequests[swapRequestCounter] = data;
        openSwapRequests.add(swapRequestCounter);
        lastSwapRequestFromAddress[data.from] = swapRequests[swapRequestCounter];
        emit NewSwapRequest(swapRequestCounter);
        return swapRequestCounter;
    }

    function newQuote(uint256 swapRequestId, address providerRefundAddress) external returns (uint256) {
        quoteCounter++;
        quotes[quoteCounter].providerRefundAddress = providerRefundAddress;
        quotes[quoteCounter].quoteExpiry = block.timestamp + swapRequests[swapRequestId].quoteDeadlineSeconds;
        quotes[quoteCounter].createdAt = block.timestamp;
        quotes[quoteCounter].feesTotal = 0;
        quotes[quoteCounter].swapRequestId = swapRequestId;

        openSwapRequests.remove(swapRequestId);    // remove the swap request from the open swap requests set
        openQuotes.add(quoteCounter);

        emit NewQuote(quoteCounter);
        return quoteCounter;
    }

    event NewSwapRequest(
        uint256 swapRequestId
    );

    event NewQuote(
        uint256 quoteId
    );

    function getSwapRequest(uint256 swapRequestId) external view returns (SwapRequest memory) {
        return swapRequests[swapRequestId];
    }

    function getQuote(uint256 quoteId) external view returns (Quote memory) {
        return quotes[quoteId];
    }

    function getRecentSwapRequests(uint256 count) external view returns (SwapRequest[] memory) {
        uint256[] memory swapRequestIds = openSwapRequests.values();
        SwapRequest[] memory recentSwapRequests = new SwapRequest[](count);
        for (uint256 i = 0; i < count; i++) {
            recentSwapRequests[i] = swapRequests[swapRequestIds[i]];
        }
        return recentSwapRequests;
    }

    function getRecentQuotes(uint256 count) external view returns (Quote[] memory) {
        uint256[] memory quoteIds = openQuotes.values();
        Quote[] memory recentQuotes = new Quote[](count);
        for (uint256 i = 0; i < count; i++) {
            recentQuotes[i] = quotes[quoteIds[i]];
        }
        return recentQuotes;
    }

    function getRequestsByIds(uint256[] memory requestIds) external view returns (SwapRequest[] memory) {
        SwapRequest[] memory requests = new SwapRequest[](requestIds.length);
        for (uint256 i = 0; i < requestIds.length; i++) {
            requests[i] = swapRequests[requestIds[i]];
        }
        return requests;
    }

    function openSwapRequestsCount() external view returns (uint256) {
        return openSwapRequests.values().length;
    }

    function openQuotesCount() external view returns (uint256) {
        return openQuotes.values().length;
    }

}
