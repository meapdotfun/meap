// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title MEAP Registry - minimal, secure agent registry for BNB Chain
/// @notice One agent per wallet; emits events for protocol indexing
contract MeapRegistry {
    event AgentRegistered(address indexed owner, bytes32 indexed agentId);
    event AgentAction(address indexed owner, bytes32 indexed agentId, string kind, string payload);
    event AgentTipped(address indexed tipper, address indexed owner, bytes32 indexed agentId, uint256 amount);

    mapping(address => bytes32) public agentIdByOwner;

    /// @notice Register an agent id for the caller (one-time)
    function register(bytes32 agentId) external {
        require(agentId != bytes32(0), "bad_id");
        require(agentIdByOwner[msg.sender] == bytes32(0), "exists");
        agentIdByOwner[msg.sender] = agentId;
        emit AgentRegistered(msg.sender, agentId);
    }

    /// @notice Log an action for the caller's agent
    function act(string calldata kind, string calldata payload) external {
        bytes32 agentId = agentIdByOwner[msg.sender];
        require(agentId != bytes32(0), "not_registered");
        emit AgentAction(msg.sender, agentId, kind, payload);
    }

    /// @notice Tip a registered agent owner (for demo purposes only)
    function tip(address owner, bytes32 agentId) external payable {
        require(agentIdByOwner[owner] == agentId && agentId != bytes32(0), "bad_agent");
        (bool ok, ) = owner.call{ value: msg.value }("");
        require(ok, "tip_fail");
        emit AgentTipped(msg.sender, owner, agentId, msg.value);
    }
}


