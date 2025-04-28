use std::fmt::Display;
use fast_pool::{Manager, Pool};
use std::ops::{Deref, DerefMut};
use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub struct TestManager {}


#[derive(Debug,Clone)]
pub struct TestConnection{
    pub inner: String,
}

impl TestConnection {
    pub fn new()->TestConnection{
        println!("new Connection");
        TestConnection{
            inner: "".to_string(),
        }
    }
}

impl Display for TestConnection{
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

impl Drop for TestConnection {
    fn drop(&mut self) {
        println!("drop Connection");
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

#[tokio::test]
async fn test_debug() {
    let p = Pool::new(TestManager {});
    println!("{:?}", p);
}

#[tokio::test]
async fn test_clone() {
    let p = Pool::new(TestManager {});
    let p2 = p.clone();
    assert_eq!(p.state(), p2.state());
}

// --nocapture
#[tokio::test]
async fn test_pool_get() {
    let p = Pool::new(TestManager {});
    p.set_max_open(10);
    let mut arr = vec![];
    for i in 0..10 {
        let v = p.get().await.unwrap();
        println!("{},{}", i, v.deref());
        arr.push(v);
    }
}

#[tokio::test]
async fn test_pool_get2() {
    let p = Pool::new(TestManager {});
    p.set_max_open(10);
    for i in 0..3 {
        let v = p.get().await.unwrap();
        println!("{},{}", i, v.deref().inner.as_str());
    }
    assert_eq!(p.state().idle, 3);
}

#[tokio::test]
async fn test_pool_get_timeout() {
    let p = Pool::new(TestManager {});
    p.set_max_open(10);
    let mut arr = vec![];
    for i in 0..10 {
        let v = p.get().await.unwrap();
        println!("{},{}", i, v.deref());
        arr.push(v);
    }
    assert_eq!(
        p.get_timeout(Some(Duration::from_secs(0))).await.is_err(),
        true
    );
}

#[tokio::test]
async fn test_pool_check() {
    let p = Pool::new(TestManager {});
    p.set_max_open(10);
    let mut v = p.get().await.unwrap();
    *v.inner.as_mut().unwrap() = TestConnection{inner:"error".to_string()};
    for _i in 0..10 {
        let v = p.get().await.unwrap();
        assert_eq!(v.deref().inner == "error", false);
    }
}

#[tokio::test]
async fn test_pool_resize() {
    let p = Pool::new(TestManager {});
    p.set_max_open(10);
    let mut arr = vec![];
    for i in 0..10 {
        let v = p.get().await.unwrap();
        println!("{},{}", i, v.deref());
        arr.push(v);
    }
    assert_eq!(
        p.get_timeout(Some(Duration::from_secs(0))).await.is_err(),
        true
    );
    p.set_max_open(11);
    assert_eq!(
        p.get_timeout(Some(Duration::from_secs(0))).await.is_err(),
        false
    );
    arr.push(p.get().await.unwrap());
    assert_eq!(
        p.get_timeout(Some(Duration::from_secs(0))).await.is_err(),
        true
    );
}

#[tokio::test]
async fn test_pool_resize2() {
    let p = Pool::new(TestManager {});
    p.set_max_open(2);
    let mut arr = vec![];
    for _i in 0..2 {
        let v = p.get().await.unwrap();
        arr.push(v);
    }
    p.set_max_open(1);
    drop(arr);
    println!("{:?}", p.state());
    assert_eq!(
        p.get_timeout(Some(Duration::from_secs(0))).await.is_err(),
        false
    );
}

#[tokio::test]
async fn test_concurrent_access() {
    let p = Pool::new(TestManager {});
    p.set_max_open(10);
    let mut handles = vec![];
    for _ in 0..10 {
        let pool = p.clone();
        let handle = tokio::spawn(async move {
            let _ = pool.get().await.unwrap();
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.await.unwrap();
    }
    assert_eq!(p.state().connections, 10);
}

#[tokio::test]
async fn test_invalid_connection() {
    let p = Pool::new(TestManager {});
    p.set_max_open(1);

    let mut conn = p.get().await.unwrap();
    //conn timeout
    *conn.inner.as_mut().unwrap() = TestConnection{inner: "error".to_string()};
    drop(conn);
    println!("pool state: {}", p.state());
    // Attempt to get a new connection, should not be the invalid one
    let new_conn = p.get().await.unwrap();
    assert_ne!(new_conn.deref().inner, "error".to_string());
}

#[tokio::test]
async fn test_connection_lifetime() {
    let p = Pool::new(TestManager {});
    p.set_max_open(10);

    let conn = p.get().await.unwrap();
    // Perform operations using the connection
    // ...

    drop(conn); // Drop the connection

    // Ensure dropped connection is not in use
    assert_eq!(p.state().in_use, 0);

    // Acquire a new connection
    let new_conn = p.get().await.unwrap();
    assert_ne!(new_conn.deref().inner, "error".to_string());
}

#[tokio::test]
async fn test_boundary_conditions() {
    let p = Pool::new(TestManager {});
    p.set_max_open(2);

    // Acquire connections until pool is full
    let conn_1 = p.get().await.unwrap();
    let _conn_2 = p.get().await.unwrap();
    println!("{}", p.state());
    assert_eq!(p.state().in_use, 2);

    // Attempt to acquire another connection (pool is full)
    assert!(p.get_timeout(Some(Duration::from_secs(0))).await.is_err());

    // Release one connection, pool is no longer full
    drop(conn_1);
    assert_eq!(p.state().in_use, 1);

    // Acquire another connection (pool has space)
    let _conn_3 = p.get().await.unwrap();
    assert_eq!(p.state().in_use, 2);

    // Increase pool size
    p.set_max_open(3);
    // Acquire another connection after increasing pool size
    let _conn_4 = p.get().await.unwrap();
    assert_eq!(p.state().in_use, 3);
}

#[tokio::test]
async fn test_pool_wait() {
    let p = Pool::new(TestManager {});
    p.set_max_open(1);
    let v = p.get().await.unwrap();
    let p1 = p.clone();
    tokio::spawn(async move {
        p1.get().await.unwrap();
        drop(p1);
    });
    let p1 = p.clone();
    tokio::spawn(async move {
        p1.get().await.unwrap();
        drop(p1);
    });
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("{:?}", p.state());
    assert_eq!(p.state().waits, 2);
    drop(v);
}

#[tokio::test]
async fn test_high_concurrency_with_timeout() {
    // Create a pool with small connection limit
    let p = Pool::new(TestManager {});
    p.set_max_open(5);
    
    // Counter for successful connection acquisition
    let success_count = Arc::new(AtomicUsize::new(0));
    // Counter for timeout events
    let timeout_count = Arc::new(AtomicUsize::new(0));
    
    // Create many concurrent tasks, exceeding pool capacity
    let mut handles = vec![];
    let task_count = 50;
    
    for _ in 0..task_count {
        let pool = p.clone();
        let success = success_count.clone();
        let timeout = timeout_count.clone();
        
        let handle = tokio::spawn(async move {
            // Use very short timeout to simulate high pressure
            match pool.get_timeout(Some(Duration::from_millis(50))).await {
                Ok(conn) => {
                    // Successfully got connection
                    success.fetch_add(1, Ordering::SeqCst);
                    // Simulate brief connection usage
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    // Return connection to pool
                    drop(conn);
                }
                Err(_) => {
                    // Connection acquisition timed out
                    timeout.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify pool state
    println!("Final pool state: {:?}", p.state());
    println!("Successful connections: {}", success_count.load(Ordering::SeqCst));
    println!("Timeouts: {}", timeout_count.load(Ordering::SeqCst));
    
    // Verify success + timeout equals total tasks
    assert_eq!(
        success_count.load(Ordering::SeqCst) + timeout_count.load(Ordering::SeqCst),
        task_count
    );
    
    // Verify pool connections did not exceed limit
    assert!(p.state().connections <= p.state().max_open);
    
    // Wait for connections to return to pool
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Pool should be in idle state, all connections in idle queue
    assert_eq!(p.state().in_use, 0);
    assert!(p.state().idle <= p.state().max_open);
}

#[tokio::test]
async fn test_concurrent_create_connection() {
    // Create a connection pool with specific connection limit
    let p = Pool::new(TestManager {});
    let max_connections = 10;
    p.set_max_open(max_connections);
    
    // Clear the connection pool
    p.set_max_open(0);
    p.set_max_open(max_connections);
    
    // Number of concurrent tasks, several times the pool limit
    let tasks = 30;
    let mut handles = vec![];
    
    // Concurrently start multiple tasks all trying to get connections
    for i in 0..tasks {
        let pool = p.clone();
        let handle = tokio::spawn(async move {
            let result = pool.get().await;
            println!("Task {} get connection: {}", i, result.is_ok());
            result
        });
        handles.push(handle);
    }
    
    // Collect results
    let mut success_count = 0;
    for handle in handles {
        if handle.await.unwrap().is_ok() {
            success_count += 1;
        }
    }
    
    // Wait for connections to return to pool
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    println!("Pool state: {:?}", p.state());
    println!("Successfully created connections: {}", success_count);
    
    // Verify pool did not exceed max connections
    assert!(p.state().connections <= max_connections);
    
    // All active connections should be returned to pool
    assert_eq!(p.state().in_use, 0);
    
    // Verify idle connections don't exceed max
    assert!(p.state().idle <= max_connections);
}

#[tokio::test]
async fn test_high_concurrency_long_connections() {
    // Create a connection pool with a specific limit
    let p = Pool::new(TestManager {});
    let max_connections = 20; // Maximum number of connections allowed
    p.set_max_open(max_connections);
    
    // Clear the connection pool
    p.set_max_open(0);
    p.set_max_open(max_connections);
    
    // Simulate a high number of concurrent requests
    let task_count = 200; // Reduced for faster test execution
    let connection_duration = Duration::from_secs(3); // Each connection lives for 3 seconds
    
    let success_count = Arc::new(AtomicUsize::new(0));
    let timeout_count = Arc::new(AtomicUsize::new(0));
    let in_progress = Arc::new(AtomicUsize::new(0));
    let max_in_progress = Arc::new(AtomicUsize::new(0));
    
    // Track the maximum number of concurrent connections
    let update_max = |current: usize, max_tracker: &Arc<AtomicUsize>| {
        let mut current_max = max_tracker.load(Ordering::Relaxed);
        while current > current_max {
            match max_tracker.compare_exchange_weak(
                current_max,
                current,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(val) => current_max = val,
            }
        }
    };
    
    println!("Starting high concurrency test with long-lived connections");
    println!("Max connections: {}, Tasks: {}, Connection duration: {:?}", 
             max_connections, task_count, connection_duration);
    
    // Create multiple concurrent tasks
    let mut handles = vec![];
    for id in 0..task_count {
        let pool = p.clone();
        let success = success_count.clone();
        let timeout = timeout_count.clone();
        let in_progress_counter = in_progress.clone();
        let max_in_progress_counter = max_in_progress.clone();
        
        let handle = tokio::spawn(async move {
            // Use timeout to prevent indefinite waiting
            match pool.get_timeout(Some(Duration::from_secs(1))).await {
                Ok(conn) => {
                    // Successfully got a connection
                    success.fetch_add(1, Ordering::SeqCst);
                    
                    // Track in-progress connections
                    let current = in_progress_counter.fetch_add(1, Ordering::SeqCst) + 1;
                    update_max(current, &max_in_progress_counter);
                    
                    println!("Task {} got connection, in-progress: {}", id, current);
                    
                    // Simulate some work with the connection
                    tokio::time::sleep(connection_duration).await;
                    
                    // Decrease in-progress counter
                    let remaining = in_progress_counter.fetch_sub(1, Ordering::SeqCst) - 1;
                    println!("Task {} completed, in-progress: {}", id, remaining);
                    
                    // Connection is automatically returned to the pool when dropped
                    drop(conn);
                }
                Err(_) => {
                    // Timed out waiting for a connection
                    timeout.fetch_add(1, Ordering::SeqCst);
                    println!("Task {} timed out waiting for connection", id);
                }
            }
        });
        
        handles.push(handle);
        
        // Small delay to simulate staggered requests
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    
    // Periodically print pool stats while waiting
    let p_status = p.clone();
    let in_progress_status = in_progress.clone();
    let status_handle = tokio::spawn(async move {
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            println!("Pool status: {:?}, In-progress: {}", 
                     p_status.state(), in_progress_status.load(Ordering::SeqCst));
        }
    });
    
    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Wait for status reporting
    let _ = status_handle.await;
    
    // Print final statistics
    println!("Connection pool stats:");
    println!("  Max connections setting: {}", max_connections);
    println!("  Total tasks: {}", task_count);
    println!("  Successful connections: {}", success_count.load(Ordering::SeqCst));
    println!("  Connection timeouts: {}", timeout_count.load(Ordering::SeqCst));
    println!("  Max concurrent connections: {}", max_in_progress.load(Ordering::SeqCst));
    println!("  Final pool state: {:?}", p.state());
    
    // Verify pool didn't exceed limits
    assert!(max_in_progress.load(Ordering::SeqCst) <= max_connections as usize);
    assert!(p.state().connections <= max_connections);
    
    // Wait for connections to be fully returned to the pool
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    // Verify all connections are idle now
    assert_eq!(p.state().in_use, 0);
}


#[tokio::test]
async fn test_concurrent_create_connection_less_for_max_open() {
    let p = Pool::new(TestManager {});
    p.set_max_open(10);
    for _ in 0..1000{
        let p1 = p.clone();
        tokio::spawn(async move {
            loop{
                let result = p1.get().await.unwrap();
                tokio::time::sleep(Duration::from_secs(1)).await;
                drop(result);
            }
        });
    }
    for _ in 0..5{
        let state = p.state();
        println!("{}", state);
        assert_eq!(state.connections <= state.max_open, true);
        assert_eq!(state.in_use <= state.max_open, true);
        assert_eq!(state.idle <= state.max_open, true);
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[tokio::test]
async fn test_change_max_open() {
    let p = Pool::new(TestManager {});
    p.set_max_open(4);
    
    let c1 = p.get().await.unwrap();
    let c2 = p.get().await.unwrap();
    let c3 = p.get().await.unwrap();
    let c4 = p.get().await.unwrap();
    
    p.set_max_open(2);
    
    drop(c1);
    drop(c2);
    drop(c3);
    drop(c4);
    
    println!("{}",p.state());
    println!("len {}",p.idle_send.len());
}

#[tokio::test]
async fn test_change_max_open2() {
    let p = Pool::new(TestManager {});
    p.set_max_open(4);

    let c1 = p.get().await.unwrap();
    let c2 = p.get().await.unwrap();
    let c3 = p.get().await.unwrap();
    let c4 = p.get().await.unwrap();

    drop(c1);
    drop(c2);
    drop(c3);
    drop(c4);
    
    p.set_max_open(2);

    println!("{}",p.state());
    println!("len {}",p.idle_send.len());
}

#[tokio::test]
async fn test_tokio_cancel() {
    let p = Pool::new(TestManager {});
    p.set_max_open(2);
    let p1 = p.clone();
    let task = tokio::spawn(async move {
        let c1 = p1.get().await.unwrap();
        let c2 = p1.get().await.unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
        drop(c1);
        drop(c2);
    });
    tokio::time::sleep(Duration::from_secs(1)).await;
    task.abort();
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("{}", p.state());
    assert_eq!(p.state().in_use, 0);
}

#[tokio::test]
async fn test_tokio_panic() {
    let p = Pool::new(TestManager {});
    p.set_max_open(2);
    let p1 = p.clone();
    let _task = tokio::spawn(async move {
        let _c1 = p1.get().await.unwrap();
        let _c2 = p1.get().await.unwrap();
        panic!("test_tokio_panic");
    });
    tokio::time::sleep(Duration::from_secs(3)).await;
    println!("{}", p.state());
    assert_eq!(p.state().in_use, 0);
}

#[tokio::test]
async fn test_timeout_zero() {
    let p = Pool::new(TestManager {});
    p.set_max_open(1);
    p.set_timeout_check(0);
    let v = p.get().await.unwrap();
    println!("{:?}",v.inner);
}

#[tokio::test]
async fn test_pool_drop() {
    let p = Pool::new(TestManager {});
    p.set_max_open(1);
    let v = p.get().await.unwrap();
    println!("{:?}",v.inner);
    drop(v);
    drop(p);
}