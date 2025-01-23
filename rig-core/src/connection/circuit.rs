//! Circuit breaker pattern for fault tolerance

use std::time::{Duration, Instant};

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    /// Circuit is closed, allowing requests
    Closed,
    /// Circuit is open, blocking requests
    Open,
    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}

/// Circuit breaker for handling connection failures
#[derive(Debug)]
pub struct CircuitBreaker {
    /// Current state of the circuit
    state: CircuitState,
    /// Number of consecutive failures
    failure_count: u32,
    /// Failure threshold before opening circuit
    threshold: u32,
    /// Last failure timestamp
    last_failure: Option<Instant>,
}

impl CircuitBreaker {
    pub fn new(threshold: u32) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            threshold,
            last_failure: None,
        }
    }

    /// Records a failure and potentially opens the circuit
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Instant::now());

        if self.failure_count >= self.threshold {
            self.state = CircuitState::Open;
        }
    }

    /// Records a success and potentially closes the circuit
    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.last_failure = None;
        self.state = CircuitState::Closed;
    }

    /// Checks if requests should be allowed
    pub fn allow_request(&self) -> bool {
        matches!(self.state, CircuitState::Closed)
    }
} 