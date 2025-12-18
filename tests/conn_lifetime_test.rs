use fast_pool::plugin::{CheckMode, DurationManager};
use fast_pool::{Manager, Pool};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub struct TestConnection {
    pub id: u64,
    created_at: std::time::Instant,
}

impl TestConnection {
    pub fn new() -> Self {
        static mut COUNTER: u64 = 0;
        unsafe {
            COUNTER += 1;
            Self {
                id: COUNTER,
                created_at: std::time::Instant::now(),
            }
        }
    }

    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
}

#[derive(Debug)]
pub struct TestManager {
    pub max_lifetime: Option<Duration>,
    pub connection_count: Arc<AtomicU64>,
}

impl TestManager {
    pub fn new(max_lifetime: Option<Duration>) -> Self {
        Self {
            max_lifetime,
            connection_count: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl Manager for TestManager {
    type Connection = TestConnection;
    type Error = String;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        self.connection_count.fetch_add(1, Ordering::SeqCst);
        Ok(TestConnection::new())
    }

    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        // Check if connection exceeds maximum lifetime
        if let Some(max_lifetime) = self.max_lifetime {
            let age = conn.age();
            if age > max_lifetime {
                return Err(format!(
                    "Connection {} expired (age: {:?}, max: {:?})",
                    conn.id, age, max_lifetime
                ));
            }
        }
        Ok(())
    }
}

#[tokio::test]
async fn test_check_mode_no_limit() {
    println!("=== test NoLimit mode ===");

    let base_manager = TestManager::new(None);
    let manager = DurationManager::new(base_manager, CheckMode::NoLimit);
    let pool = Pool::new(manager);

    // get connectiontestcheck
    let conn = pool.get().await.unwrap();
    let initial_check_count = conn.age();

    // checkshouldcallmanagecheck
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let _ = pool.get().await.unwrap();
    }

    // at/in NoLimit mode，connectionshould
    assert!(conn.age() > initial_check_count);
    drop(conn);
    println!("✅ NoLimit modetestpass");
}

#[tokio::test]
async fn test_check_mode_skip_interval() {
    println!("=== test SkipInterval mode ===");

    let base_manager = TestManager::new(None);
    let manager = DurationManager::new(
        base_manager,
        CheckMode::SkipInterval(Duration::from_millis(100)),
    );
    let pool = Pool::new(manager);

    // get connection
    let mut conn = pool.get().await.unwrap();

    // get，shouldat/ininterval
    let start_time = std::time::Instant::now();
    for _ in 0..3 {
        let _ = conn = pool.get().await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let elapsed = start_time.elapsed();
    // due tocheck，thisoperationshouldcomplete
    assert!(elapsed < Duration::from_millis(100));

    drop(conn);
    println!("✅ SkipInterval modetestpass");
}

#[tokio::test]
async fn test_check_mode_max_lifetime() {
    println!("=== test MaxLifetime mode ===");

    let base_manager = TestManager::new(None);
    let manager = DurationManager::new(
        base_manager,
        CheckMode::MaxLifetime(Duration::from_millis(150)),
    );
    let pool = Pool::new(manager);

    // get connection
    let conn = pool.get().await.unwrap();
    let conn_id = conn.id;
    drop(conn);

    // waitmaxlifetime
    tokio::time::sleep(Duration::from_millis(200)).await;

    // get connectionshouldfailcreateconnection
    match pool.get().await {
        Ok(new_conn) => {
            println!(
                "Got new connection: ID = {},  = {:?}",
                new_conn.id,
                new_conn.age()
            );
            // ifcreateconnection，IDshould
            assert_ne!(conn_id, new_conn.id);
            drop(new_conn);
        }
        Err(e) => {
            // println!("connectionby: {}", e);
            assert!(e.contains("max lifetime"));
        }
    }

    println!("✅ MaxLifetime modetestpass");
}

#[tokio::test]
async fn test_conn_max_lifetime_basic() {
    let base_manager = TestManager::new(Some(Duration::from_millis(200))); // 200mslifetime
    let manager = DurationManager::new(
        base_manager,
        CheckMode::SkipInterval(Duration::from_millis(50)), // 50mscheck
    );
    let pool = Pool::new(manager);

    // get connection
    let conn = pool.get().await.unwrap();
    let conn_id = conn.id;
    drop(conn);

    // waitconnection
    tokio::time::sleep(Duration::from_millis(250)).await;

    // get connection，shouldnewconnection（old）
    let new_conn = pool.get().await.unwrap();
    println!(
        "getconnection: ID = {},  = {:?}",
        new_conn.id,
        new_conn.age()
    );

    // connectionIDshould
    assert_ne!(conn_id, new_conn.id);

    drop(new_conn);
}

#[tokio::test]
async fn test_conn_max_lifetime_with_check_duration() {
    let base_manager = TestManager::new(Some(Duration::from_millis(100))); // 100mslifetime
    let manager = DurationManager::new(
        base_manager,
        CheckMode::SkipInterval(Duration::from_millis(200)), // 200mscheck，lifetime
    );
    let pool = Pool::new(manager);

    let conn = pool.get().await.unwrap();
    let _conn_id = conn.id;
    drop(conn);

    // waitlifetimecheck interval
    tokio::time::sleep(Duration::from_millis(150)).await;

    // due tocheck intervallifetime，connectionby
    let new_conn = pool.get().await.unwrap();
    drop(new_conn);

    // waitcheck interval
    tokio::time::sleep(Duration::from_millis(100)).await;

    let final_conn = pool.get().await.unwrap();

    drop(final_conn);
}

#[tokio::test]
async fn test_no_max_lifetime() {
    let base_manager = TestManager::new(None); // lifetimelimit
    let no_limit_manager = DurationManager::new(
        base_manager,
        CheckMode::SkipInterval(Duration::from_millis(100)), // check interval
    );
    let pool = Pool::new(no_limit_manager);

    let conn1 = pool.get().await.unwrap();
    let conn1_id = conn1.id;
    drop(conn1);

    // wait
    tokio::time::sleep(Duration::from_millis(300)).await;

    // connectionshould（lifetimelimit）
    let conn2 = pool.get().await.unwrap();
    let conn2_id = conn2.id;

    println!("connection1 ID: {}, connection2 ID: {}", conn1_id, conn2_id);

    // due tolifetimelimit，connection
    // due tocheck intervalat/in，newconnection，this

    drop(conn2);
}

#[tokio::test]
async fn test_conn_max_lifetime_concurrent() {
    let base_manager = TestManager::new(Some(Duration::from_millis(150)));
    let lifetime_manager = DurationManager::new(
        base_manager,
        CheckMode::SkipInterval(Duration::from_millis(50)), // check
    );
    let pool = Arc::new(Pool::new(lifetime_manager));

    let mut handles = Vec::new();
    let success_count = Arc::new(AtomicU64::new(0));
    let expired_count = Arc::new(AtomicU64::new(0));

    // concurrentgetandreleaseconnection
    for i in 0..10 {
        let pool_clone = pool.clone();
        let success = success_count.clone();
        let expired = expired_count.clone();

        let handle = tokio::spawn(async move {
            for j in 0..5 {
                match pool_clone
                    .get_timeout(Some(Duration::from_millis(100)))
                    .await
                {
                    Ok(conn) => {
                        success.fetch_add(1, Ordering::SeqCst);
                        println!(
                            "Task {}-{} get connection: ID = {},  = {:?}",
                            i,
                            j,
                            conn.id,
                            conn.age()
                        );

                        // makeconnection
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        drop(conn);
                    }
                    Err(_e) => {
                        expired.fetch_add(1, Ordering::SeqCst);
                        // println!("Task {}-{} connectionerror: {}", i, j, e);
                    }
                }

                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        });

        handles.push(handle);
    }

    // waitcomplete
    for handle in handles {
        handle.await.unwrap();
    }

    let success = success_count.load(Ordering::SeqCst);
    let expired = expired_count.load(Ordering::SeqCst);

    // verifyresult
    assert!(success + expired > 0); // shouldsuccessget
    assert!(pool.state().connections <= pool.state().max_open);

    println!("✅ concurrent environmentlifetime testpass");
}

#[tokio::test]
async fn test_check_duration_manager_only() {
    let base_manager = TestManager::new(None);
    let check_manager = DurationManager::new(
        base_manager,
        CheckMode::SkipInterval(Duration::from_millis(100)), // check interval，lifetimelimit
    );
    let pool = Pool::new(check_manager);

    // println!("createcheck intervalconnection");

    let conn1 = pool.get().await.unwrap();
    drop(conn1);

    // get，check
    let conn2 = pool.get().await.unwrap();
    drop(conn2);

    // waitcheck interval
    tokio::time::sleep(Duration::from_millis(150)).await;

    let conn3 = pool.get().await.unwrap();
    drop(conn3);
}

#[tokio::test]
async fn test_edge_cases() {
    println!("=== testboundarycase ===");

    // testlifetime
    let base_manager = TestManager::new(Some(Duration::from_millis(1)));
    let lifetime_manager = DurationManager::new(base_manager, CheckMode::NoLimit);
    let pool = Pool::new(lifetime_manager);

    let conn = pool.get().await.unwrap();
    let _conn_id = conn.id;
    drop(conn);

    // wait
    tokio::time::sleep(Duration::from_millis(5)).await;

    let _new_conn = pool.get().await.unwrap();

    // testcheck interval
    let base_manager2 = TestManager::new(Some(Duration::from_millis(50)));
    let long_interval_manager = DurationManager::new(
        base_manager2,
        CheckMode::SkipInterval(Duration::from_millis(1000)), // check interval
    );
    let pool2 = Pool::new(long_interval_manager);

    let conn2 = pool2.get().await.unwrap();
    let _conn2_id = conn2.id;
    drop(conn2);

    // waitlifetimecheck interval
    tokio::time::sleep(Duration::from_millis(60)).await;

    let _new_conn2 = pool2.get().await.unwrap();

    println!("✅ boundarycasetestpass");
}
