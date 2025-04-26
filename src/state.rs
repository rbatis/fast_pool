use std::fmt::{Display, Formatter};

#[derive(Debug, Eq, PartialEq)]
pub struct State {
    /// max open limit
    pub max_open: u64,
    /// connections = in_use number + idle number + connecting number
    pub connections: u64,
    /// user use connection number
    pub in_use: u64,
    /// idle connection
    pub idle: u64,
    /// wait get connections number
    pub waits: u64,
    /// connecting connections
    pub connecting: u64,
    /// checking connections
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
