use std::time::Duration;
use std::sync::Arc;
use fast_pool::{Manager, Pool};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug)]
pub struct TestConnection {
    pub id: u64,
}

impl TestConnection {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        println!("创建新连接: {}", id);
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

    // 设置最大连接数和最大空闲连接数
    pool.set_max_open(10);
    pool.set_max_idle_conns(5);

    // 获取 8 个连接
    let mut connections = Vec::new();
    for i in 0..8 {
        let conn = pool.get().await.unwrap();
        println!("获取连接 {}: ID = {}", i + 1, conn.id);
        connections.push(conn);
    }

    assert_eq!(pool.state().connections, 8);
    assert_eq!(pool.state().in_use, 8);
    assert_eq!(pool.state().idle, 0);

    // 释放所有连接
    drop(connections);

    // 等待连接回到池中
    tokio::time::sleep(Duration::from_millis(10)).await;

    // 由于设置了最大空闲连接数为 5，应该只有 5 个连接保留在池中
    println!("释放后状态: {}", pool.state());
    assert!(pool.state().idle <= 5);
    assert!(pool.state().connections <= 5);
}

#[tokio::test]
async fn test_set_max_idle_conns_dynamic() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    pool.set_max_open(10);
    pool.set_max_idle_conns(3);

    // 创建 5 个连接
    let mut connections = Vec::new();
    for _ in 0..5 {
        connections.push(pool.get().await.unwrap());
    }
    assert_eq!(pool.state().connections, 5);
    assert_eq!(pool.state().in_use, 5);

    // 释放所有连接
    drop(connections);
    tokio::time::sleep(Duration::from_millis(10)).await;

    // 应该只有 3 个空闲连接
    assert!(pool.state().idle <= 3);
    assert!(pool.state().connections <= 3);

    // 动态调整最大空闲连接数
    pool.set_max_idle_conns(1);

    // 等待调整生效
    tokio::time::sleep(Duration::from_millis(10)).await;

    // 应该只有 1 个空闲连接
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

    // 并发获取和释放连接
    for i in 0..15 {
        let pool_clone = pool.clone();
        let success = success_count.clone();
        let handle = tokio::spawn(async move {
            for j in 0..3 {
                match pool_clone.get_timeout(Some(Duration::from_millis(50))).await {
                    Ok(conn) => {
                        success.fetch_add(1, Ordering::SeqCst);
                        println!("Task {}-{} 获取连接: {}", i, j, conn.id);
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        drop(conn);
                    }
                    Err(_) => {
                        println!("Task {}-{} 获取连接超时", i, j);
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        handles.push(handle);
    }

    // 等待所有任务完成
    for handle in handles {
        handle.await.unwrap();
    }

    // 验证连接池状态
    let state = pool.state();
    println!("最终状态: {}", state);

    // 验证空闲连接数不超过最大限制
    assert!(state.idle <= 8);
    assert!(state.connections <= state.max_open);
}

#[tokio::test]
async fn test_get_max_idle_conns() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    // 默认情况下，max_idle 应该等于 max_open
    assert_eq!(pool.get_max_idle_conns(), 32);
    assert_eq!(pool.get_max_open(), 32);

    // 设置不同的值
    pool.set_max_open(15);
    pool.set_max_idle_conns(7);

    assert_eq!(pool.get_max_open(), 15);
    assert_eq!(pool.get_max_idle_conns(), 7);

    // 设置 max_open 为小于 max_idle 的值，应该自动调整 max_idle
    pool.set_max_open(5);

    // 检查 max_open 是否正确更新
    assert_eq!(pool.get_max_open(), 5);
    // max_idle 应该被自动调整为不超过 max_open
    assert_eq!(pool.get_max_idle_conns(), 5);
}

#[tokio::test]
async fn test_max_idle_conns_edge_cases() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    // 测试设置为 0
    pool.set_max_idle_conns(0);

    // 获取一个连接然后释放
    let conn = pool.get().await.unwrap();
    drop(conn);

    tokio::time::sleep(Duration::from_millis(10)).await;

    // 空闲连接数应该为 0
    assert_eq!(pool.state().idle, 0);

    // 测试设置为很大的值
    pool.set_max_idle_conns(1000);
    pool.set_max_open(5);

    let mut connections = Vec::new();
    for _ in 0..5 {
        connections.push(pool.get().await.unwrap());
    }

    drop(connections);
    tokio::time::sleep(Duration::from_millis(10)).await;

    // 空闲连接数不应该超过 max_open
    assert!(pool.state().idle <= 5);
}

#[tokio::test]
async fn test_max_open_zero_early_return() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    let original_max_open = pool.get_max_open();

    // 尝试设置为 0，这应该导致早期返回 (pool.rs line 162-163)
    pool.set_max_open(0);

    // 验证值没有改变
    assert_eq!(pool.get_max_open(), original_max_open);

    // 设置正常值应该生效
    pool.set_max_open(10);
    assert_eq!(pool.get_max_open(), 10);
}

#[tokio::test]
async fn test_set_max_open_force_cleanup_loop() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    // 创建多个连接并让它们变为空闲状态
    let mut guards = vec![];
    for _ in 0..5 {
        guards.push(pool.get().await.unwrap());
    }

    // 释放所有连接，让它们进入空闲队列
    for guard in guards {
        drop(guard);
    }

    // 给一点时间让连接被回收
    tokio::time::sleep(Duration::from_millis(10)).await;

    // 检查状态，确认有空闲连接
    let state = pool.state();
    assert!(state.idle > 0);

    // 将 max_open 设置为非常小的值，强制触发清理循环 (pool.rs line 171)
    pool.set_max_open(1);

    // 验证设置生效
    assert_eq!(pool.get_max_open(), 1);
}

#[tokio::test]
async fn test_set_max_open_aggressive_cleanup() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    // 创建大量连接
    let mut connections = vec![];
    for _ in 0..20 {
        connections.push(pool.get().await.unwrap());
    }

    // 同时获取 max_open 状态
    let initial_max_open = pool.get_max_open();
    assert_eq!(initial_max_open, 32); // 默认值

    // 释放一半连接
    for conn in connections.into_iter().take(10) {
        drop(conn);
    }

    // 给连接回收一些时间
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 将 max_open 设置为需要清理很多连接的值
    // 这会强制触发 loop 内的多轮清理 (pool.rs line 171)
    pool.set_max_open(5);

    // 验证设置生效
    assert_eq!(pool.get_max_open(), 5);

    // 验证 max_idle 也被相应调整
    assert_eq!(pool.get_max_idle_conns(), 5);
}

#[tokio::test]
async fn test_set_max_idle_conns_zero_validation() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    let original_max_idle = pool.get_max_idle_conns();

    // 测试 set_max_idle_conns 设置为 0
    // 与 set_max_open 不同，set_max_idle_conns 没有对 0 值的早期返回验证
    pool.set_max_idle_conns(0);

    // 验证是否设置了 0 值（这暴露了缺少输入验证的问题）
    assert_eq!(pool.get_max_idle_conns(), 0);

    // 恢复正常值
    pool.set_max_idle_conns(original_max_idle);
    assert_eq!(pool.get_max_idle_conns(), original_max_idle);
}

#[tokio::test]
async fn test_max_idle_exceeds_max_open_state_inconsistency() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    // 首先设置一个较小的 max_open
    pool.set_max_open(5);
    assert_eq!(pool.get_max_open(), 5);
    assert_eq!(pool.get_max_idle_conns(), 5); // set_max_open 应该自动调整 max_idle

    // 现在，通过 set_max_idle_conns 设置一个超过 max_open 的值
    // 这暴露了状态不一致问题：max_idle > max_open
    pool.set_max_idle_conns(10);

    // 验证不一致状态：max_idle 确实可以超过 max_open
    assert_eq!(pool.get_max_open(), 5);
    assert_eq!(pool.get_max_idle_conns(), 10); // 这个值大于 max_open，是不一致的

    // 测试这种不一致状态下的行为 - 使用较少的连接避免卡死
    let mut connections = vec![];
    for _ in 0..3 {
        connections.push(pool.get().await.unwrap());
    }

    // 释放连接
    drop(connections);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 即使 max_idle 设置为 10，实际的空闲连接不应该超过 max_open (5)
    let state = pool.state();
    assert!(state.idle <= 3, "实际空闲连接数不应该超过创建的连接数");
    assert!(state.connections <= 5, "总连接数不应该超过 max_open");
}

#[tokio::test]
async fn test_set_max_idle_conns_cleanup_robustness() {
    let manager = TestManager::new();
    let pool = Pool::new(manager);

    pool.set_max_open(10);
    pool.set_max_idle_conns(5);

    // 创建一些连接
    let mut connections = vec![];
    for _ in 0..3 {
        connections.push(pool.get().await.unwrap());
    }

    // 释放连接让它们进入空闲队列
    drop(connections);
    tokio::time::sleep(Duration::from_millis(10)).await;

    let initial_state = pool.state();
    println!("设置前的状态: {}", initial_state);

    // 设置一个更小的值，触发清理逻辑
    pool.set_max_idle_conns(1);

    // 等待清理生效
    tokio::time::sleep(Duration::from_millis(10)).await;

    let final_state = pool.state();
    println!("设置后的状态: {}", final_state);

    // 验证清理逻辑是否正确处理了 try_recv 失败的情况
    // 当前实现没有检查 try_recv 的返回值，可能导致 connections 计数不准确
    assert!(final_state.idle <= 1, "空闲连接数应该 <= 1");

    // 验证 connections 计数是否准确
    // idle + in_use 应该等于 connections
    assert_eq!(
        final_state.idle + final_state.in_use,
        final_state.connections,
        "connections 计数不准确：idle + in_use = {}, connections = {}",
        final_state.idle + final_state.in_use,
        final_state.connections
    );
}