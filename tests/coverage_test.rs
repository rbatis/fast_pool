use fast_pool::{Pool, Manager, ConnectionGuard};
use fast_pool::duration::AtomicDuration;
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct TestManager {
    pub connection_count: AtomicU64,
}

#[derive(Debug)]
pub struct TestConnection {
    pub id: u32,
}

impl Manager for TestManager {
    type Connection = TestConnection;
    type Error = String;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        self.connection_count.fetch_add(1, Ordering::SeqCst);
        Ok(TestConnection {
            id: self.connection_count.load(Ordering::SeqCst) as u32,
        })
    }

    async fn check(&self, _conn: &mut Self::Connection) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[tokio::test]
async fn test_atomic_duration_edge_cases() {
    // Test AtomicDuration::new with None
    let atomic_none = AtomicDuration::new(None);
    assert_eq!(atomic_none.get(), None);

    // Test AtomicDuration::new with Some duration
    let atomic_some = AtomicDuration::new(Some(Duration::from_millis(100)));
    assert_eq!(atomic_some.get(), Some(Duration::from_millis(100)));

    // Test AtomicDuration::take - extracts value and sets to None
    let atomic_take = AtomicDuration::new(Some(Duration::from_millis(200)));
    let taken = atomic_take.take();
    assert_eq!(taken, Some(Duration::from_millis(200)));
    assert_eq!(atomic_take.get(), None);

    // Test AtomicDuration::take on empty value
    let atomic_empty = AtomicDuration::new(None);
    let taken_none = atomic_empty.take();
    assert_eq!(taken_none, None);
    assert_eq!(atomic_empty.get(), None);

    // Test AtomicDuration::into_inner
    let atomic_into = AtomicDuration::new(Some(Duration::from_millis(300)));
    let inner = atomic_into.into_inner();
    assert_eq!(inner, Some(Duration::from_millis(300)));

    // Test AtomicDuration::store with None
    let atomic_store = AtomicDuration::new(Some(Duration::from_millis(400)));
    atomic_store.store(None);
    assert_eq!(atomic_store.get(), None);

    // Test AtomicDuration::store with Some
    atomic_store.store(Some(Duration::from_millis(500)));
    assert_eq!(atomic_store.get(), Some(Duration::from_millis(500)));
}

#[tokio::test]
async fn test_connection_guard_debug_format() {
    let manager = TestManager::default();
    let pool = Pool::new(manager);

    let conn = TestConnection { id: 1 };
    let guard = ConnectionGuard::new(conn, pool.clone());

    // Test the Debug formatting (covers the Debug trait impl)
    let debug_str = format!("{:?}", guard);
    assert!(debug_str.contains("ConnectionGuard"));
}

#[tokio::test]
async fn test_connection_guard_drop_scenarios() {
    let manager = TestManager::default();
    let pool = Pool::new(manager);

    // Test dropping an unchecked connection (covers line 55-57 in guard.rs)
    {
        let conn = TestConnection { id: 2 };
        let _guard = ConnectionGuard::new(conn, pool.clone());
        // Don't set checked, so when guard drops it should decrement connections
    }
    // Guard is dropped here, should hit the unchecked branch

    // Test dropping a checked connection
    {
        let conn = TestConnection { id: 3 };
        let mut _guard = ConnectionGuard::new(conn, pool.clone());
        _guard.set_checked(true);
        // This will go through the checked branch on drop
    }
}

#[tokio::test]
async fn test_pool_set_max_open_cleanup_logic() {
    let manager = TestManager::default();
    let pool = Pool::new(manager);

    // Get some connections first
    let conn1 = pool.get().await.unwrap();
    let conn2 = pool.get().await.unwrap();
    let conn3 = pool.get().await.unwrap();

    // Now reduce max_open to trigger cleanup logic (lines 171-179 in pool.rs)
    pool.set_max_open(1);

    // Verify max_open was set correctly
    assert_eq!(pool.get_max_open(), 1);

    // The cleanup logic should have removed excess idle connections
    assert_eq!(pool.get_max_idle_conns(), 1);

    // Return connections
    drop(conn1);
    drop(conn2);
    drop(conn3);
}

#[tokio::test]
async fn test_pool_timeout_check_methods() {
    let manager = TestManager::default();
    let pool = Pool::new(manager);

    // Test get_timeout_check (covers lines 221-223 in pool.rs)
    let timeout = pool.get_timeout_check();
    assert_eq!(timeout, Some(Duration::from_secs(10)));

    // Test set_timeout_check with None
    pool.set_timeout_check(None);
    assert_eq!(pool.get_timeout_check(), None);

    // Test set_timeout_check with custom duration
    let custom_timeout = Duration::from_secs(30);
    pool.set_timeout_check(Some(custom_timeout));
    assert_eq!(pool.get_timeout_check(), Some(custom_timeout));
}

#[tokio::test]
async fn test_pool_edge_cases() {
    let manager = TestManager::default();
    let pool = Pool::new(manager);

    // Test setting max_open to 0 (should return early, covers line 163)
    pool.set_max_open(0);
    assert_ne!(pool.get_max_open(), 0); // Should not have changed

    // Test normal max_open setting
    pool.set_max_open(5);
    assert_eq!(pool.get_max_open(), 5);
}

#[test]
fn test_atomic_duration_concurrent_operations() {
    use std::sync::Arc;
    use std::thread;

    let atomic = Arc::new(AtomicDuration::new(Some(Duration::from_millis(100))));
    let mut handles = vec![];

    // Spawn multiple threads to test concurrent take operations
    for _ in 0..10 {
        let atomic_clone = atomic.clone();
        let handle = thread::spawn(move || {
            let _ = atomic_clone.take();
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Final value should be None after all takes
    assert_eq!(atomic.get(), None);
}