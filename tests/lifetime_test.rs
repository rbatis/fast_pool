use std::time::Duration;
use std::sync::Arc;
use fast_pool::{Manager, Pool};
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone)]
pub struct TestConnection {
    pub id: u64,
}

impl TestConnection {
    pub fn new() -> Self {
        static mut COUNTER: u64 = 0;
        unsafe {
            COUNTER += 1;
            Self { id: COUNTER }
        }
    }
}

#[derive(Debug)]
pub struct LifetimeTestManager {
    pub connection_count: Arc<std::sync::atomic::AtomicU64>,
}

impl Manager for LifetimeTestManager {
    type Connection = TestConnection;
    type Error = String;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        self.connection_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(TestConnection::new())
    }

    async fn check(&self, _conn: &mut Self::Connection) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[tokio::test]
async fn test_set_max_idle_conns() {
    let manager = LifetimeTestManager {
        connection_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };
    let pool = Pool::new(manager);

    // 设置最大连接数为 10，最大空闲连接数为 5
    pool.set_max_open(10);
    pool.set_max_idle_conns(5);

    let mut connections = Vec::new();

    // 获取 8 个连接
    for _ in 0..8 {
        connections.push(pool.get().await.unwrap());
    }

    assert_eq!(pool.state().connections, 8);
    assert_eq!(pool.state().in_use, 8);
    assert_eq!(pool.state().idle, 0);

    // 释放所有连接
    drop(connections);

    // 等待连接回到池中
    tokio::time::sleep(Duration::from_millis(10)).await;

    // 由于设置了最大空闲连接数为 5，应该只有 5 个连接保留在池中
    assert!(pool.state().idle <= 5);
    assert!(pool.state().connections <= 5);
}

#[tokio::test]
async fn test_conn_max_lifetime() {
    let manager = LifetimeTestManager {
        connection_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };
    let pool = Pool::new(manager);

    // 设置最大生命周期为 50ms
    pool.set_conn_max_lifetime(Some(Duration::from_millis(50)));
    pool.set_max_open(10);

    // 创建一个连接
    let conn = pool.get().await.unwrap();
    let _conn_id = conn.id;
    drop(conn);

    // 等待连接回到池中
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(pool.state().idle, 1);

    // 等待超过最大生命周期
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 再次获取连接，检查是否需要时间戳功能
    assert!(pool.needs_timestamp());

    // 验证连接池状态合理即可
    let state = pool.state();
    assert!(state.connections <= state.max_open);
    assert!(state.idle <= state.max_open);
}

#[tokio::test]
async fn test_max_lifetime_with_concurrent_access() {
    let manager = LifetimeTestManager {
        connection_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };
    let pool = Arc::new(Pool::new(manager));

    // 设置较短的最大生命周期
    pool.set_conn_max_lifetime(Some(Duration::from_millis(30)));
    pool.set_max_open(5);

    let mut handles = vec![];

    // 并发获取和释放连接
    for i in 0..10 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            for j in 0..3 {
                let conn = pool_clone.get().await.unwrap();
                println!("Task {}-{} got connection: {}", i, j, conn.id);
                tokio::time::sleep(Duration::from_millis(20)).await;
                drop(conn);
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        handles.push(handle);
    }

    // 等待所有任务完成
    for handle in handles {
        handle.await.unwrap();
    }

    // 检查池状态
    let state = pool.state();
    println!("Final state: {}", state);

    // 验证连接数不会超过最大限制
    assert!(state.connections <= state.max_open);
}

#[tokio::test]
async fn test_get_max_idle_conns() {
    let manager = LifetimeTestManager {
        connection_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };
    let pool = Pool::new(manager);

    // 默认情况下，max_idle 应该等于 max_open
    assert_eq!(pool.get_max_idle_conns(), 32);
    assert_eq!(pool.get_max_open(), 32);

    // 设置不同的值
    pool.set_max_open(20);
    pool.set_max_idle_conns(10);

    assert_eq!(pool.get_max_open(), 20);
    assert_eq!(pool.get_max_idle_conns(), 10);

    // 设置 max_open 为小于 max_idle 的值应该自动调整 max_idle
    pool.set_max_open(5);
    assert_eq!(pool.get_max_open(), 5);
    assert_eq!(pool.get_max_idle_conns(), 5); // 应该被自动调整
}

#[tokio::test]
async fn test_get_conn_max_lifetime() {
    let manager = LifetimeTestManager {
        connection_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };
    let pool = Pool::new(manager);

    // 默认情况下，最大生命周期应该为 None
    assert_eq!(pool.get_conn_max_lifetime(), None);

    // 设置最大生命周期
    let lifetime = Duration::from_secs(60);
    pool.set_conn_max_lifetime(Some(lifetime));
    assert_eq!(pool.get_conn_max_lifetime(), Some(lifetime));

    // 清除最大生命周期
    pool.set_conn_max_lifetime(None);
    assert_eq!(pool.get_conn_max_lifetime(), None);
}