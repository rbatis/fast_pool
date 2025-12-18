use fast_pool::{Manager, Pool};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub struct TestConnection {
    pub id: u64,
}

impl TestConnection {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        // println!("Creating new connection: {}", id);
        Self { id }
    }
}

#[derive(Debug)]
pub struct TestManager {
    pub connection_count: Arc<AtomicU64>,
}

impl TestManager {
    pub fn new() -> Self {
        Self {
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

    async fn check(&self, _conn: &mut Self::Connection) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[tokio::test]
async fn test_set_max_idle_conns_basic() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    // setmaxconnectionandmaxidleconnection
    pool.set_max_open(10);
    pool.set_max_idle_conns(5);

    // get 8 connection
    let mut connections = Vec::new();
    for i in 0..8 {
        let conn = pool.get().await.unwrap();
        println!("get connection {}: ID = {}", i + 1, conn.id);
        connections.push(conn);
    }

    assert_eq!(pool.state().connections, 8);
    assert_eq!(pool.state().in_use, 8);
    assert_eq!(pool.state().idle, 0);

    // releaseconnection
    drop(connections);

    // waitconnection
    tokio::time::sleep(Duration::from_millis(10)).await;

    // due tosetmaxidleconnectionas 5，should 5 connectionat/in
    assert!(pool.state().idle <= 5);
    assert!(pool.state().connections <= 5);
}

#[tokio::test]
async fn test_set_max_idle_conns_dynamic() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    pool.set_max_open(10);
    pool.set_max_idle_conns(3);

    // create 5 connection
    let mut connections = Vec::new();
    for _ in 0..5 {
        connections.push(pool.get().await.unwrap());
    }
    assert_eq!(pool.state().connections, 5);
    assert_eq!(pool.state().in_use, 5);

    // releaseconnection
    drop(connections);
    tokio::time::sleep(Duration::from_millis(10)).await;

    // should 3 idleconnection
    assert!(pool.state().idle <= 3);
    assert!(pool.state().connections <= 3);

    // adjustmaxidleconnection
    pool.set_max_idle_conns(1);

    // waitadjust
    tokio::time::sleep(Duration::from_millis(10)).await;

    // should 1 idleconnection
    assert!(pool.state().idle <= 1);
    assert!(pool.state().connections <= 1);
}

#[tokio::test]
async fn test_set_max_idle_conns_with_concurrent_access() {
    let manager = TestManager::new();
    let pool = Arc::new(Pool::new(manager));

    pool.set_max_open(20);
    pool.set_max_idle_conns(8);

    let mut handles = Vec::new();
    let success_count = Arc::new(AtomicU64::new(0));

    // concurrentgetandreleaseconnection
    for i in 0..15 {
        let pool_clone = pool.clone();
        let success = success_count.clone();
        let handle = tokio::spawn(async move {
            for j in 0..3 {
                match pool_clone
                    .get_timeout(Some(Duration::from_millis(50)))
                    .await
                {
                    Ok(conn) => {
                        success.fetch_add(1, Ordering::SeqCst);
                        println!("Task {}-{} get connection: {}", i, j, conn.id);
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        drop(conn);
                    }
                    Err(_) => {
                        println!("Task {}-{} get connectiontimeout", i, j);
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        handles.push(handle);
    }

    // waitcomplete
    for handle in handles {
        handle.await.unwrap();
    }

    // verifyconnectionstate
    let state = pool.state();

    // verifyidleconnectionmaxlimit
    assert!(state.idle <= 8);
    assert!(state.connections <= state.max_open);
}

#[tokio::test]
async fn test_get_max_idle_conns() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    // case，max_idle should max_open
    assert_eq!(pool.get_max_idle_conns(), 32);
    assert_eq!(pool.get_max_open(), 32);

    // setvalue
    pool.set_max_open(15);
    pool.set_max_idle_conns(7);

    assert_eq!(pool.get_max_open(), 15);
    assert_eq!(pool.get_max_idle_conns(), 7);

    // set max_open as max_idle value，shouldadjust max_idle
    pool.set_max_open(5);

    // check max_open
    assert_eq!(pool.get_max_open(), 5);
    // max_idle shouldbyadjustas max_open
    assert_eq!(pool.get_max_idle_conns(), 5);
}

#[tokio::test]
async fn test_max_idle_conns_edge_cases() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    // testsetas 0
    pool.set_max_idle_conns(0);

    // get connectionthenrelease
    let conn = pool.get().await.unwrap();
    drop(conn);

    tokio::time::sleep(Duration::from_millis(10)).await;

    // idleconnectionshouldas 0
    assert_eq!(pool.state().idle, 0);

    // testsetasvalue
    pool.set_max_idle_conns(1000);
    pool.set_max_open(5);

    let mut connections = Vec::new();
    for _ in 0..5 {
        connections.push(pool.get().await.unwrap());
    }

    drop(connections);
    tokio::time::sleep(Duration::from_millis(10)).await;

    // idleconnectionshould max_open
    assert!(pool.state().idle <= 5);
}

#[tokio::test]
async fn test_max_open_zero_early_return() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    let original_max_open = pool.get_max_open();

    // trysetas 0，thisshouldreturn (pool.rs line 162-163)
    pool.set_max_open(0);

    // verifyvalue
    assert_eq!(pool.get_max_open(), original_max_open);

    // setvalueshould
    pool.set_max_open(10);
    assert_eq!(pool.get_max_open(), 10);
}

#[tokio::test]
async fn test_set_max_open_force_cleanup_loop() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    // createconnectionletasidlestate
    let mut guards = vec![];
    for _ in 0..5 {
        guards.push(pool.get().await.unwrap());
    }

    // releaseconnection，letidle
    for guard in guards {
        drop(guard);
    }

    // giveletconnectionby
    tokio::time::sleep(Duration::from_millis(10)).await;

    // checkstate，confirmidleconnection
    let state = pool.state();
    assert!(state.idle > 0);

    // will max_open setasvalue，cleanup (pool.rs line 171)
    pool.set_max_open(1);

    // verifyset
    assert_eq!(pool.get_max_open(), 1);
}

#[tokio::test]
async fn test_set_max_open_aggressive_cleanup() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    // createconnection
    let mut connections = vec![];
    for _ in 0..20 {
        connections.push(pool.get().await.unwrap());
    }

    // at the same timeget max_open state
    let initial_max_open = pool.get_max_open();
    assert_eq!(initial_max_open, 32); // value

    // releaseconnection
    for conn in connections.into_iter().take(10) {
        drop(conn);
    }

    // giveconnection
    tokio::time::sleep(Duration::from_millis(50)).await;

    // will max_open setascleanupconnectionvalue
    // this loop cleanup (pool.rs line 171)
    pool.set_max_open(5);

    // verifyset
    assert_eq!(pool.get_max_open(), 5);

    // verify max_idle byadjust
    assert_eq!(pool.get_max_idle_conns(), 5);
}

#[tokio::test]
async fn test_set_max_idle_conns_zero_validation() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    let original_max_idle = pool.get_max_idle_conns();

    // test set_max_idle_conns setas 0
    // with set_max_open ，set_max_idle_conns to 0 valuereturnverify
    pool.set_max_idle_conns(0);

    // verifyset 0 value（thisverify）
    assert_eq!(pool.get_max_idle_conns(), 0);

    // value
    pool.set_max_idle_conns(original_max_idle);
    assert_eq!(pool.get_max_idle_conns(), original_max_idle);
}

#[tokio::test]
async fn test_max_idle_exceeds_max_open_state_inconsistency() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    // firstset max_open
    pool.set_max_open(5);
    assert_eq!(pool.get_max_open(), 5);
    assert_eq!(pool.get_max_idle_conns(), 5); // set_max_open shouldadjust max_idle

    // at/in，pass set_max_idle_conns set max_open value
    // thisstate：max_idle > max_open
    pool.set_max_idle_conns(10);

    // verifystate：max_idle  max_open
    assert_eq!(pool.get_max_open(), 5);
    assert_eq!(pool.get_max_idle_conns(), 10); // thisvalue max_open，

    // testthisstateas - makeconnection
    let mut connections = vec![];
    for _ in 0..3 {
        connections.push(pool.get().await.unwrap());
    }

    // releaseconnection
    drop(connections);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // make max_idle setas 10，idleconnectionshould max_open (5)
    let state = pool.state();
    assert!(state.idle <= 3, "idleconnectionshouldcreateconnection");
    assert!(state.connections <= 5, "connectionshould max_open");
}

#[tokio::test]
async fn test_set_max_idle_conns_cleanup_robustness() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    pool.set_max_open(10);
    pool.set_max_idle_conns(5);

    // createconnection
    let mut connections = vec![];
    for _ in 0..3 {
        connections.push(pool.get().await.unwrap());
    }

    // releaseconnectionletidle
    drop(connections);
    tokio::time::sleep(Duration::from_millis(10)).await;

    let _initial_state = pool.state();

    // setvalue，cleanup
    pool.set_max_idle_conns(1);

    // waitcleanup
    tokio::time::sleep(Duration::from_millis(10)).await;

    let final_state = pool.state();

    // verifycleanuphandle try_recv failcase
    // whencheck try_recv returnvalue， connections
    assert!(final_state.idle <= 1, "idleconnectionshould <= 1");

    // verify connections
    // idle + in_use should connections
    assert_eq!(
        final_state.idle + final_state.in_use,
        final_state.connections,
        "connections ：idle + in_use = {}, connections = {}",
        final_state.idle + final_state.in_use,
        final_state.connections
    );
}
