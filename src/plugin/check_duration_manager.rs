use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crate::duration::AtomicDuration;
use crate::Manager;

/// `CheckDurationConnectionManager` is a manager wrapper that implements connection validation
/// based on a specified idle duration.
///
/// This manager only performs actual connection validation after a specified duration has passed
/// since the last check. This can significantly reduce the overhead of frequent validation
/// for connections that are used repeatedly within short time intervals.
///
/// # Example
/// ```no_run
/// use std::time::Duration;
/// use fast_pool::{Manager, Pool};
/// use fast_pool::plugin::CheckDurationConnectionManager;
/// 
/// // Assume we have some database manager that implements Manager trait
/// struct MyDatabaseManager;
/// 
/// impl Manager for MyDatabaseManager {
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
/// let base_manager = MyDatabaseManager;
/// let duration_manager = CheckDurationConnectionManager::new(base_manager, Duration::from_secs(60));
/// let pool = Pool::new(duration_manager);
/// ```
pub struct CheckDurationConnectionManager<M: Manager> {
    /// The underlying connection manager that handles the actual connection operations
    pub manager: M,
    /// Minimum duration between actual connection checks
    pub duration: Duration,
    /// Timestamp of the last actual check (stored as duration since UNIX epoch)
    pub instant: AtomicDuration,
}

impl<M: Manager> CheckDurationConnectionManager<M> {
    /// Creates a new `CheckDurationConnectionManager` with the specified manager and check duration.
    ///
    /// # Parameters
    /// - `manager`: The underlying connection manager
    /// - `duration`: The minimum duration that must pass before performing an actual check
    ///
    /// # Returns
    /// A new `CheckDurationConnectionManager` instance
    pub fn new(manager: M, duration: Duration) -> Self {
        Self {
            manager,
            duration,
            instant: AtomicDuration::new(Some(Duration::from_secs(0))),
        }
    }
}

impl<M: Manager> Manager for CheckDurationConnectionManager<M> {
    type Connection = M::Connection;
    type Error = M::Error;

    /// Creates a new connection by delegating to the underlying manager.
    ///
    /// This method simply forwards the connection request to the wrapped manager.
    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        self.manager.connect().await
    }

    /// Checks if a connection is still valid, but only performs actual validation
    /// if the specified duration has passed since the last check.
    ///
    /// # Logic
    /// 1. Get the current time (as duration since UNIX epoch)
    /// 2. Get the timestamp of the last check
    /// 3. If less than `duration` time has passed, assume the connection is valid
    /// 4. Otherwise, perform an actual check using the underlying manager and update the timestamp
    ///
    /// # Parameters
    /// - `conn`: The connection to check
    ///
    /// # Returns
    /// - `Ok(())` if the connection is valid or if the duration hasn't passed
    /// - The error from the underlying manager if the check fails
    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0));
        let last_check = self.instant.get().unwrap_or_default();
        // Skip the actual check if not enough time has passed
        if now.saturating_sub(last_check) < self.duration {
            return Ok(());
        }
        // Update the timestamp and perform the actual check
        self.instant.store(Some(now));
        self.manager.check(conn).await
    }
} 