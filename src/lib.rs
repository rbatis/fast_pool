#![allow(async_fn_in_trait)]

#[macro_use]
mod defer;
pub mod duration;
pub mod guard;
pub mod plugin;
pub mod pool;
pub mod state;

/// Trait for connection management and validation
pub trait Manager: std::any::Any + Send + Sync {
    type Connection;

    type Error: for<'a> From<&'a str>;

    /// Create new connection
    async fn connect(&self) -> Result<Self::Connection, Self::Error>;

    /// Validate connection is alive (return Error if connection should be dropped)
    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error>;
}

pub use guard::ConnectionGuard;
pub use pool::Pool;
pub use state::State;
