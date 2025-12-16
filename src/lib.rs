#![allow(async_fn_in_trait)]

#[macro_use]
mod defer;
mod duration;
pub mod state;
pub mod guard;
pub mod pool;
pub mod plugin;
pub mod time_connection;

/// Manager create Connection and check Connection
pub trait Manager {
    type Connection;

    type Error: for<'a> From<&'a str>;

    ///create Connection and check Connection
    async fn connect(&self) -> Result<Self::Connection, Self::Error>;
    ///check Connection is alive? if not return Error(Connection will be drop)
    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error>;
}

pub use pool::Pool;
pub use guard::ConnectionGuard;
pub use state::State;

