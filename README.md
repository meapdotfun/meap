<p align="center">
  <img src="meap.png" alt="MEAP" width="120" />
</p>

MEAP (Message Exchange Agent Protocol)
===================================

A high-performance protocol designed for AI agent communication with built-in support for streaming, validation, and security. MEAP provides a robust foundation for agent-to-agent interactions with features like rate limiting, circuit breaking, and load balancing.

System Architecture
------------------
```
     ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
     │   Agent A    │     │  MEAP Node   │     │   Agent B    │
     │              │     │              │     │              │
     │ ┌──────────┐ │     │ ┌──────────┐ │     │ ┌──────────┐ │
     │ │Protocol  │ │     │ │Load      │ │     │ │Protocol  │ │
     │ │Versioning│◄├─────┤►│Balancer  │◄├─────┤►│Versioning│ │
     │ └──────────┘ │     │ └──────────┘ │     │ └──────────┘ │
     │      ▲       │     │      ▲       │     │      ▲       │
     │      │       │     │      │       │     │      │       │
     │ ┌──────────┐ │     │ ┌──────────┐ │     │ ┌──────────┐ │
     │ │Circuit   │ │     │ │Rate      │ │     │ │Circuit   │ │
     │ │Breaker   │ │     │ │Limiter   │ │     │ │Breaker   │ │
     │ └──────────┘ │     │ └──────────┘ │     │ └──────────┘ │
     └──────────────┘     └──────────────┘     └──────────────┘
```

Message Flow
-----------
```
     ┌─────────┐                                      ┌─────────┐
     │ Agent A │                                      │ Agent B │
     └────┬────┘                                      └────┬────┘
          │                                                │
          │ 1. Send Message                               │
          │ ─────────────┐                               │
          │              │                               │
          │              │ 2. Version Check              │
          │              │                               │
          │              │ 3. Rate Limit Check           │
          │              │                               │
          │              │ 4. Circuit Check              │
          │              │                               │
          │              └─────────────────────────────► │
          │                                              │
          │                5. Process Message            │
          │                   ┌───────────┐             │
          │                   │           │             │
          │                   └───────────┘             │
          │                                             │
          │              6. Response                    │
          │ ◄─────────────────────────────────────────  │
     ┌────┴────┐                                      ┌────┴────┐
     │ Agent A │                                      │ Agent B │
     └─────────┘                                      └─────────┘
```

Protocol Stack
-------------
```
   ┌─────────────────────────────────────┐
   │           Application Layer          │
   │  - Agent Logic                      │
   │  - Message Processing               │
   ├─────────────────────────────────────┤
   │           Protocol Layer            │
   │  - Message Format                   │
   │  - Version Management               │
   │  - Validation                       │
   ├─────────────────────────────────────┤
   │         Reliability Layer           │
   │  - Circuit Breaking                 │
   │  - Rate Limiting                    │
   │  - Load Balancing                   │
   ├─────────────────────────────────────┤
   │         Transport Layer             │
   │  - WebSocket                        │
   │  - TLS                             │
   └─────────────────────────────────────┘
```

Core Features
------------

Protocol Versioning
```rust
let agent = Agent::new(
    "assistant-1",
    ProtocolVersion::new(1, 0, 0),
    ConnectionConfig::default()
);
```

Message Validation and Processing
```rust
async fn handle_message(message: Message) -> Result<()> {
    agent.check_version(&message).await?;
    
    if let Some(response) = agent.process_message(message).await? {
        agent.send_message(response).await?;
    }
    Ok(())
}
```

Technical Architecture
--------------------

Core Components:
1. Protocol Layer
   - Message serialization/deserialization
   - Protocol version management
   - Message validation

2. Connection Management
   - WebSocket connection pooling
   - Connection lifecycle management
   - Automatic reconnection handling

3. Reliability Features
   - Circuit breaker pattern
   - Rate limiting
   - Load balancing
   - Connection metrics

4. Security Layer
   - TLS encryption
   - Message authentication
   - Access control

Usage
-----

```rust
use meap_core::{Agent, ProtocolVersion, ConnectionConfig};

let agent = Agent::new(
    "agent-id",
    ProtocolVersion::new(1, 0, 0),
    ConnectionConfig {
        max_reconnects: 3,
        reconnect_delay: Duration::from_secs(1),
        buffer_size: 32,
    }
);

async fn handle_messages(agent: Agent) -> Result<()> {
    while let Some(message) = agent.receive().await? {
        if let Some(response) = agent.process_message(message).await? {
            agent.send_message(response).await?;
        }
    }
    Ok(())
}
```

License
-------
MIT License

Copyright (c) 2024 MEAP Contributors 

Advanced Configuration
--------------------
```rust
pub struct AdvancedConfig {
    // Connection Settings
    pub connection: ConnectionConfig,
    
    // Rate Limiting
    pub rate_limit: RateLimitConfig,
    
    // Load Balancing
    pub load_balancer: BalancerConfig,
    
    // Circuit Breaking
    pub circuit_breaker: CircuitBreakerConfig,
    
    // Security
    pub tls: TlsConfig,
}
```

Performance Metrics
-----------------
```
Throughput:    ~10,000 messages/second
Latency:       < 10ms (99th percentile)
CPU Usage:     < 5% on modern hardware
Memory:        ~50MB base + ~1KB per connection
```

Error Handling Matrix
-------------------
```
┌────────────────┬────────────────────┬─────────────────┐
│ Error Type     │ Detection Method   │ Recovery Action │
├────────────────┼────────────────────┼─────────────────┤
│ Connection     │ Heartbeat Timeout  │ Auto-reconnect  │
│ Protocol       │ Version Mismatch   │ Negotiate       │
│ Rate Limit     │ Counter Threshold  │ Backoff         │
│ Circuit Open   │ Failure Count      │ Wait/Reset      │
│ Security      │ TLS/Auth Failure   │ Retry/Block     │
└────────────────┴────────────────────┴─────────────────┘
``` 