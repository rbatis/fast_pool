use std::fmt::{Display, Formatter};

/// Current state of the connection pool
#[derive(Debug, Eq, PartialEq)]
pub struct State {
    /// Maximum open connections allowed
    pub max_open: u64,
    /// Total connections = in_use + idle + connecting
    pub connections: u64,
    /// Currently active connections
    pub in_use: u64,
    /// Idle connections available
    pub idle: u64,
    /// Connections waiting to be acquired
    pub waits: u64,
    /// Currently establishing connections
    pub connecting: u64,
    /// Currently being checked/validated
    pub checking: u64,
}

impl Display for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ max_open: {}, connections: {}, in_use: {}, idle: {}, connecting: {}, checking: {}, waits: {} }}",
            self.max_open, self.connections, self.in_use, self.idle, self.connecting, self.checking, self.waits
        )
    }
}
