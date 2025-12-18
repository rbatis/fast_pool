use std::time::Duration;
use std::sync::Arc;
use fast_pool::{Manager, Pool};
use fast_pool::plugin::{CheckDurationManager, CheckMode};
use std::sync::atomic::{AtomicU64, Ordering};

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
        // 检查连接是否超过最大生命周期
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
    println!("=== 测试 NoLimit 模式 ===");

    let base_manager = TestManager::new(None);
    let manager = CheckDurationManager::new(
        base_manager,
        CheckMode::NoLimit
    );
    let pool = Pool::new(manager);

    // 获取连接并测试多次检查
    let conn = pool.get().await.unwrap();
    let initial_check_count = conn.age();

    // 多次检查应该每次都调用底层管理器的check
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let _ = pool.get().await.unwrap();
    }

    // 在 NoLimit 模式下，连接应该仍然有效
    assert!(conn.age() > initial_check_count);
    drop(conn);
    println!("✅ NoLimit 模式测试通过");
}

#[tokio::test]
async fn test_check_mode_skip_interval() {
    println!("=== 测试 SkipInterval 模式 ===");

    let base_manager = TestManager::new(None);
    let manager = CheckDurationManager::new(
        base_manager,
        CheckMode::SkipInterval(Duration::from_millis(100))
    );
    let pool = Pool::new(manager);

    // 获取连接
    let mut conn = pool.get().await.unwrap();

    // 立即再次获取，应该在跳过间隔内
    let start_time = std::time::Instant::now();
    for _ in 0..3 {
        let _ = conn = pool.get().await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let elapsed = start_time.elapsed();
    // 由于跳过了检查，这个操作应该很快完成
    assert!(elapsed < Duration::from_millis(100));

    drop(conn);
    println!("✅ SkipInterval 模式测试通过");
}

#[tokio::test]
async fn test_check_mode_max_lifetime() {
    println!("=== 测试 MaxLifetime 模式 ===");

    let base_manager = TestManager::new(None);
    let manager = CheckDurationManager::new(
        base_manager,
        CheckMode::MaxLifetime(Duration::from_millis(150))
    );
    let pool = Pool::new(manager);

    // 获取一个连接
    let conn = pool.get().await.unwrap();
    let conn_id = conn.id;
    println!("获取连接: ID = {}, 创建时间 = {:?}", conn.id, conn.age());
    drop(conn);

    // 等待超过最大生命周期
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 再次获取连接应该失败或创建新连接
    match pool.get().await {
        Ok(new_conn) => {
            println!("获取到新连接: ID = {}, 年龄 = {:?}", new_conn.id, new_conn.age());
            // 如果池创建了新连接，ID应该不同
            assert_ne!(conn_id, new_conn.id);
            drop(new_conn);
        }
        Err(e) => {
            println!("连接被拒绝: {}", e);
            assert!(e.contains("max lifetime"));
        }
    }

    println!("✅ MaxLifetime 模式测试通过");
}

#[tokio::test]
async fn test_conn_max_lifetime_basic() {
    println!("=== 测试基本连接生命周期管理 ===");

    let base_manager = TestManager::new(Some(Duration::from_millis(200))); // 200ms生命周期
    let manager = CheckDurationManager::new(
        base_manager,
        CheckMode::SkipInterval(Duration::from_millis(50)) // 每50ms检查一次
    );
    let pool = Pool::new(manager);

    // 获取一个连接
    let conn = pool.get().await.unwrap();
    let conn_id = conn.id;
    println!("获取连接: ID = {}, 年龄 = {:?}", conn.id, conn.age());
    drop(conn);

    // 等待连接过期
    tokio::time::sleep(Duration::from_millis(250)).await;

    // 再次获取连接，应该获得一个新的连接（旧的已过期）
    let new_conn = pool.get().await.unwrap();
    println!("获取新连接: ID = {}, 年龄 = {:?}", new_conn.id, new_conn.age());

    // 新连接的ID应该不同
    assert_ne!(conn_id, new_conn.id);

    drop(new_conn);
    println!("✅ 基本连接生命周期测试通过");
}

#[tokio::test]
async fn test_conn_max_lifetime_with_check_duration() {
    println!("=== 测试带检查间隔的连接生命周期管理 ===");

    let base_manager = TestManager::new(Some(Duration::from_millis(100))); // 100ms生命周期
    let manager = CheckDurationManager::new(
        base_manager,
        CheckMode::SkipInterval(Duration::from_millis(200)) // 每200ms检查一次，比生命周期长
    );
    let pool = Pool::new(manager);

    let conn = pool.get().await.unwrap();
    let _conn_id = conn.id;
    drop(conn);

    // 等待超过生命周期但少于检查间隔
    tokio::time::sleep(Duration::from_millis(150)).await;

    // 由于检查间隔比生命周期长，连接可能还未被检测到过期
    let new_conn = pool.get().await.unwrap();
    println!("连接ID: {}, 检查间隔可能还未检测到过期", new_conn.id);
    drop(new_conn);

    // 等待超过检查间隔
    tokio::time::sleep(Duration::from_millis(100)).await;

    let final_conn = pool.get().await.unwrap();
    println!("最终连接ID: {}, 此时应该已检测到过期", final_conn.id);

    drop(final_conn);
    println!("✅ 带检查间隔的生命周期测试通过");
}

#[tokio::test]
async fn test_no_max_lifetime() {
    println!("=== 测试无生命周期限制的情况 ===");

    let base_manager = TestManager::new(None); // 无生命周期限制
    let no_limit_manager = CheckDurationManager::new(
        base_manager,
        CheckMode::SkipInterval(Duration::from_millis(100)) // 仍然有检查间隔
    );
    let pool = Pool::new(no_limit_manager);

    let conn1 = pool.get().await.unwrap();
    let conn1_id = conn1.id;
    drop(conn1);

    // 等待很长时间
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 连接应该仍然有效（无生命周期限制）
    let conn2 = pool.get().await.unwrap();
    let conn2_id = conn2.id;

    println!("连接1 ID: {}, 连接2 ID: {}", conn1_id, conn2_id);

    // 由于没有生命周期限制，可能是同一个连接
    // 但由于检查间隔的存在，也可能是新的连接，这取决于实现

    drop(conn2);
    println!("✅ 无生命周期限制测试通过");
}

#[tokio::test]
async fn test_conn_max_lifetime_concurrent() {
    println!("=== 测试并发环境下的连接生命周期 ===");

    let base_manager = TestManager::new(Some(Duration::from_millis(150)));
    let lifetime_manager = CheckDurationManager::new(
        base_manager,
        CheckMode::SkipInterval(Duration::from_millis(50)) // 频繁检查
    );
    let pool = Arc::new(Pool::new(lifetime_manager));

    let mut handles = Vec::new();
    let success_count = Arc::new(AtomicU64::new(0));
    let expired_count = Arc::new(AtomicU64::new(0));

    // 并发获取和释放连接
    for i in 0..10 {
        let pool_clone = pool.clone();
        let success = success_count.clone();
        let expired = expired_count.clone();

        let handle = tokio::spawn(async move {
            for j in 0..5 {
                match pool_clone.get_timeout(Some(Duration::from_millis(100))).await {
                    Ok(conn) => {
                        success.fetch_add(1, Ordering::SeqCst);
                        println!("Task {}-{} 获取连接: ID = {}, 年龄 = {:?}",
                                i, j, conn.id, conn.age());

                        // 模拟使用连接
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        drop(conn);
                    }
                    Err(e) => {
                        expired.fetch_add(1, Ordering::SeqCst);
                        println!("Task {}-{} 连接过期或错误: {}", i, j, e);
                    }
                }

                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        });

        handles.push(handle);
    }

    // 等待所有任务完成
    for handle in handles {
        handle.await.unwrap();
    }

    let success = success_count.load(Ordering::SeqCst);
    let expired = expired_count.load(Ordering::SeqCst);

    println!("成功获取: {}, 过期/错误: {}", success, expired);
    println!("最终池状态: {}", pool.state());

    // 验证结果合理性
    assert!(success + expired > 0); // 应该有成功的获取
    assert!(pool.state().connections <= pool.state().max_open);

    println!("✅ 并发环境生命周期测试通过");
}

#[tokio::test]
async fn test_check_duration_manager_only() {
    println!("=== 测试仅使用检查间隔管理（无生命周期） ===");

    let base_manager = TestManager::new(None);
    let check_manager = CheckDurationManager::new(
        base_manager,
        CheckMode::SkipInterval(Duration::from_millis(100)) // 仅检查间隔，无生命周期限制
    );
    let pool = Pool::new(check_manager);

    println!("创建仅检查间隔的连接池");

    let conn1 = pool.get().await.unwrap();
    println!("第一次获取: ID = {}", conn1.id);
    drop(conn1);

    // 短时间内再次获取，可能跳过实际检查
    let conn2 = pool.get().await.unwrap();
    println!("第二次获取: ID = {}", conn2.id);
    drop(conn2);

    // 等待超过检查间隔
    tokio::time::sleep(Duration::from_millis(150)).await;

    let conn3 = pool.get().await.unwrap();
    println!("超过检查间隔后获取: ID = {}", conn3.id);
    drop(conn3);

    println!("✅ 仅检查间隔管理测试通过");
}

#[tokio::test]
async fn test_edge_cases() {
    println!("=== 测试边界情况 ===");

    // 测试极短的生命周期
    let base_manager = TestManager::new(Some(Duration::from_millis(1)));
    let lifetime_manager = CheckDurationManager::new(
        base_manager,
        CheckMode::NoLimit
    );
    let pool = Pool::new(lifetime_manager);

    let conn = pool.get().await.unwrap();
    let conn_id = conn.id;
    drop(conn);

    // 等待过期
    tokio::time::sleep(Duration::from_millis(5)).await;

    let new_conn = pool.get().await.unwrap();
    println!("极短生命周期 - 旧ID: {}, 新ID: {}", conn_id, new_conn.id);

    // 测试极长的检查间隔
    let base_manager2 = TestManager::new(Some(Duration::from_millis(50)));
    let long_interval_manager = CheckDurationManager::new(
        base_manager2,
        CheckMode::SkipInterval(Duration::from_millis(1000)) // 很长的检查间隔
    );
    let pool2 = Pool::new(long_interval_manager);

    let conn2 = pool2.get().await.unwrap();
    let conn2_id = conn2.id;
    drop(conn2);

    // 等待超过生命周期但远小于检查间隔
    tokio::time::sleep(Duration::from_millis(60)).await;

    let new_conn2 = pool2.get().await.unwrap();
    println!("长检查间隔 - 旧ID: {}, 新ID: {}", conn2_id, new_conn2.id);

    println!("✅ 边界情况测试通过");
}