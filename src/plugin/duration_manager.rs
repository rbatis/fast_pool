use crate::Manager;
use atomic::Atomic;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::Ordering;
use std::{
    sync::atomic::AtomicI8,
    time::{Duration, Instant},
};

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

impl CheckMode {
    /// Convert mode to i8 for atomic storage
    fn as_i8(&self) -> i8 {
        match self {
            CheckMode::NoLimit => 0,
            CheckMode::SkipInterval(_) => 1,
            CheckMode::MaxLifetime(_) => 2,
        }
    }

    /// Convert duration to atomic nanoseconds for storage
    fn as_duration(&self) -> Atomic<u128> {
        match self {
            CheckMode::NoLimit => Atomic::new(Duration::from_secs(0).as_nanos()),
            CheckMode::SkipInterval(duration) => Atomic::new(duration.clone().as_nanos()),
            CheckMode::MaxLifetime(duration) => Atomic::new(duration.clone().as_nanos()),
        }
    }

    /// Reconstruct CheckMode from stored atomic values
    fn new(mode: i8, duration: u128) -> Self {
        let secs = (duration / 1_000_000_000) as u64;
        let nanos = (duration % 1_000_000_000) as u32;
        match mode {
            0 => CheckMode::NoLimit,
            1 => CheckMode::SkipInterval(Duration::new(secs, nanos)),
            2 => CheckMode::MaxLifetime(Duration::new(secs, nanos)),
            _ => CheckMode::NoLimit,
        }
    }
}

/// Atomic storage for CheckMode configuration
pub struct CheckModeAtomic {
    pub mode: AtomicI8,
    pub duration: Atomic<u128>,
}

impl CheckModeAtomic {
    /// Create new atomic check mode storage
    pub fn new(mode: CheckMode) -> Self {
        let mode_value: i8 = mode.as_i8();
        let duration = mode.as_duration();
        Self {
            mode: AtomicI8::new(mode_value),
            duration: duration,
        }
    }

    /// Update check mode atomically
    pub fn set_mode(&self, mode: CheckMode) {
        self.mode.store(mode.as_i8(), Ordering::Relaxed);
        self.duration.store(
            mode.as_duration().load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }

    /// Get current check mode
    pub fn get_mode(&self) -> CheckMode {
        let mode = self.mode.load(Ordering::Relaxed);
        let duration = self.duration.load(Ordering::Relaxed);
        CheckMode::new(mode, duration)
    }
}

/// Connection wrapper with creation timestamp for lifetime tracking
pub struct DurationConnection<T> {
    inner: T,
    instant: Option<Instant>,
}

impl<T> Deref for DurationConnection<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for DurationConnection<T> {
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
/// use fast_pool::plugin::{DurationManager, CheckMode};
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
/// let manager = DurationManager::new(MyManager, CheckMode::SkipInterval(Duration::from_secs(30)));
/// let pool = Pool::new(manager);
/// ```
pub struct DurationManager<M: Manager> {
    /// The underlying connection manager
    pub manager: M,
    /// Check strategy mode
    pub mode: CheckModeAtomic,
}

impl<M: Manager> DurationManager<M> {
    /// Create new DurationManager with check mode
    pub fn new(manager: M, mode: CheckMode) -> Self {
        Self {
            manager,
            mode: CheckModeAtomic::new(mode),
        }
    }
}

impl<M: Manager> Manager for DurationManager<M> {
    type Connection = DurationConnection<M::Connection>;
    type Error = M::Error;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        Ok(DurationConnection {
            inner: self.manager.connect().await?,
            instant: {
                match self.mode.get_mode() {
                    CheckMode::NoLimit => None,
                    CheckMode::SkipInterval(_) => Some(Instant::now()),
                    CheckMode::MaxLifetime(_) => Some(Instant::now()),
                }
            },
        })
    }

    /// Check connection based on configured mode strategy
    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        match &self.mode.get_mode() {
            CheckMode::NoLimit => {
                // Always perform the underlying check
            }
            CheckMode::SkipInterval(duration) => {
                // Skip if within check interval
                if let Some(instant) = conn.instant.as_ref() {
                    if instant.elapsed() < *duration {
                        return Ok(());
                    }
                }
            }
            CheckMode::MaxLifetime(duration) => {
                // Fail if connection exceeded max lifetime
                if let Some(instant) = conn.instant.as_ref() {
                    if instant.elapsed() > *duration {
                       return Err(M::Error::from("connection exceeded max lifetime"));
                    }
                }
            }
        }
        self.manager.check(conn).await
    }
}


impl<M: Manager> Deref for DurationManager<M> {
    type Target = M;
    fn deref(&self) -> &Self::Target {
        &self.manager
    }
}

impl<M: Manager> DerefMut for DurationManager<M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.manager
    }
}