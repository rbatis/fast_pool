use std::fmt::Display;
use std::ops::{Deref, DerefMut};
use std::time::Duration;
use fast_pool::{Manager, Pool};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// Import the test connection and manager from pool_test.rs
#[derive(Debug,Clone)]
pub struct TestManager {}

#[derive(Debug, Clone)]
pub struct TestConnection {
    pub inner: String,
}

impl TestConnection {
    pub fn new() -> TestConnection {
        TestConnection {
            inner: "".to_string(),
        }
    }
}

impl Display for TestConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl Deref for TestConnection {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for TestConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Manager for TestManager {
    type Connection = TestConnection;
    type Error = String;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        Ok(TestConnection::new())
    }

    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        if conn.inner != "" {
            return Err(Self::Error::from(&conn.to_string()));
        }
        Ok(())
    }
}

/// Wrapper for TestManager to track check calls
#[derive(Clone)]
pub struct CheckCounterManager {
    manager: TestManager,
    check_count: Arc<AtomicUsize>,
}

impl CheckCounterManager {
    fn new() -> Self {
        Self {
            manager: TestManager{},
            check_count: Arc::new(AtomicUsize::new(0)),
        }
    }
    
    fn get_check_count(&self) -> usize {
        self.check_count.load(Ordering::SeqCst)
    }
}

impl Manager for CheckCounterManager {
    type Connection = TestConnection;
    type Error = String;
    
    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        self.manager.connect().await
    }
    
    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        self.check_count.fetch_add(1, Ordering::SeqCst);
        self.manager.check(conn).await
    }
}

/// CheckDurationConnectionManager implementation
struct CheckDurationConnectionManager<M: Manager> {
    pub manager: M,
    pub duration: Duration,
    pub last_check: std::sync::Mutex<Option<std::time::SystemTime>>,
}

impl<M: Manager> CheckDurationConnectionManager<M> {
    fn new(manager: M, duration: Duration) -> Self {
        Self {
            manager,
            duration,
            last_check: std::sync::Mutex::new(None),
        }
    }
}

impl<M: Manager> Manager for CheckDurationConnectionManager<M> {
    type Connection = M::Connection;
    type Error = M::Error;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        self.manager.connect().await
    }

    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        let now = std::time::SystemTime::now();
        let should_check = {
            let mut last_check = self.last_check.lock().unwrap();
            let should_check = match *last_check {
                None => true,
                Some(time) => {
                    match now.duration_since(time) {
                        Ok(elapsed) => elapsed >= self.duration,
                        Err(_) => true, // Clock went backwards, check to be safe
                    }
                }
            };
            
            if should_check {
                *last_check = Some(now);
            }
            
            should_check
        };
        
        if should_check {
            self.manager.check(conn).await
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn test_check_duration_manager_basic() {
    let counter_manager = CheckCounterManager::new();
    // Create a duration manager that only checks every 500ms
    let duration_manager = CheckDurationConnectionManager::new(counter_manager.clone(), Duration::from_millis(500));
    let pool = Pool::new(duration_manager);
    
    // Get a connection and immediately return it to the pool
    let conn = pool.get().await.unwrap();
    drop(conn);
    assert_eq!(counter_manager.get_check_count(), 1, "First check should happen");
    
    // Get another connection immediately - should not trigger a check
    let conn = pool.get().await.unwrap();
    drop(conn);
    assert_eq!(counter_manager.get_check_count(), 1, "Second immediate check should be skipped");
    
    // Wait for duration to expire
    tokio::time::sleep(Duration::from_millis(550)).await;
    
    // Get a connection after duration - should trigger a check
    let conn = pool.get().await.unwrap();
    drop(conn);
    assert_eq!(counter_manager.get_check_count(), 2, "Check should happen after duration");
}

#[tokio::test]
async fn test_check_duration_manager_concurrent() {
    let counter_manager = CheckCounterManager::new();
    let check_count_tracker = counter_manager.check_count.clone();
    
    // Create a duration manager with a longer check interval
    let duration_manager = CheckDurationConnectionManager::new(counter_manager, Duration::from_millis(200));
    let pool = Pool::new(duration_manager);
    
    // Get and release 10 connections rapidly
    for _ in 0..10 {
        let conn = pool.get().await.unwrap();
        drop(conn);
    }
    
    // Only one check should have happened despite 10 connections
    assert_eq!(check_count_tracker.load(Ordering::SeqCst), 1, 
        "Multiple rapid connections should only trigger one check");
    
    // Wait for duration to expire
    tokio::time::sleep(Duration::from_millis(250)).await;
    
    // Get one more connection after waiting
    let conn = pool.get().await.unwrap();
    drop(conn);
    assert_eq!(check_count_tracker.load(Ordering::SeqCst), 2, 
        "Check should happen after duration expires");
}

#[tokio::test]
async fn test_check_duration_manager_invalid_connection() {
    let counter_manager = CheckCounterManager::new();
    let check_count_tracker = counter_manager.check_count.clone();
    
    // Create a duration manager with a moderate check interval
    let duration_manager = CheckDurationConnectionManager::new(counter_manager, Duration::from_millis(100));
    let pool = Pool::new(duration_manager);
    
    // Get a connection and make it invalid
    let mut conn = pool.get().await.unwrap();
    (*conn).inner = "invalid".to_string();
    drop(conn); // Return invalid connection to pool
    
    // The first check should have happened
    assert_eq!(check_count_tracker.load(Ordering::SeqCst), 1);
    
    // Get a new connection immediately - shouldn't trigger another check due to duration
    // NOTE: Since our manager skips the check, the invalid connection will be returned
    // without validation. This is expected behavior for the CheckDurationConnectionManager!
    let conn = pool.get().await.unwrap();
    
    // We expect the connection to still be invalid since checks are being skipped
    assert_eq!(conn.inner.as_ref().unwrap().inner, "invalid", 
        "Connection should still be invalid since check was skipped");
    
    // Check counter should remain the same since we're within the duration
    assert_eq!(check_count_tracker.load(Ordering::SeqCst), 1,
        "Check should be skipped due to duration limit");
    
    // Now wait for duration to pass
    tokio::time::sleep(Duration::from_millis(150)).await;
    
    // Return the invalid connection
    drop(conn);
    
    // Get another connection, now it should be checked and fixed
    let conn = pool.get().await.unwrap();
    // Now the connection should be valid because the check should have happened
    // due to time expiration
    assert_eq!(check_count_tracker.load(Ordering::SeqCst), 2,
        "Check should happen after duration expires");
    
    // The connection should be valid now
    assert_eq!(conn.inner.as_ref().unwrap().inner, "", 
        "Connection should be valid after check is performed");
} 