use std::time::{Duration, Instant};
use crate::Manager;
use std::ops::{Deref, DerefMut};

/// Connection check modes
#[derive(Debug, Clone)]
pub enum CheckMode {
    /// No check interval limit - always check
    NoLimit,
    /// Skip checks for specified duration after each check
    SkipInterval(Duration),
    /// Force connection error if exceeded maximum lifetime
    MaxLifetime(Duration),
}


pub struct DurationConnection<T>{
    inner:T,
    instant:Instant,
}

impl <T>Deref for DurationConnection<T>{
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl <T>DerefMut for DurationConnection<T>{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }

}

/// Connection manager that limits check frequency to reduce overhead.
///
/// Wraps another manager and only performs actual connection validation
/// based on the specified check mode.
///
/// # Example
/// ```no_run
/// use std::time::Duration;
/// use fast_pool::{Manager, Pool};
/// use fast_pool::plugin::{CheckDurationManager, CheckMode};
///
/// struct MyManager;
///
/// impl Manager for MyManager {
///     type Connection = ();
///     type Error = String;
///
///     async fn connect(&self) -> Result<Self::Connection, Self::Error> {
///         Ok(())
///     }
///
///     async fn check(&self, _conn: &mut Self::Connection) -> Result<(), Self::Error> {
///         Ok(())
///     }
/// }
///
/// let manager = CheckDurationManager::new(MyManager, CheckMode::SkipInterval(Duration::from_secs(30)));
/// let pool = Pool::new(manager);
/// ```
pub struct CheckDurationManager<M: Manager> {
    /// The underlying connection manager
    pub manager: M,
    /// Check strategy mode
    pub mode: CheckMode,
}

impl<M: Manager> CheckDurationManager<M> {
    /// Creates a new `CheckDurationManager`.
    ///
    /// # Parameters
    /// - `manager`: The underlying connection manager
    /// - `mode`: The check strategy mode
    pub fn new(manager: M, mode: CheckMode) -> Self {
        Self {
            manager,
            mode,
        }
    }
}

impl<M: Manager> Manager for CheckDurationManager<M> {
    type Connection = DurationConnection<M::Connection>;
    type Error = M::Error;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        Ok(DurationConnection{
           inner: self.manager.connect().await?,
           instant: Instant::now(),
        })
    }

    /// Checks connection validity based on the configured mode.
    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        match &self.mode {
            CheckMode::NoLimit => {
                //do nothing
            }
            CheckMode::SkipInterval(duration) => {
                // Skip check if not enough time has passed
                if conn.instant.elapsed() < *duration {
                    return Ok(());
                }
            }
            CheckMode::MaxLifetime(max_lifetime) => {
                // Check if connection exceeded maximum lifetime
                if conn.instant.elapsed() > *max_lifetime {
                    return Err(M::Error::from("connection exceeded max lifetime"));
                }
            }
        }
        self.manager.check(conn).await
    }
} 