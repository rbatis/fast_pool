use std::time::Duration;
use fast_pool::{Manager, Pool};
use fast_pool::plugin::CheckDurationConnectionManager;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug)]
pub struct ExampleConnection {
    id: u64,
    created_at: Option<std::time::Instant>,
}

impl ExampleConnection {
    pub fn new() -> Self {
        static mut COUNTER: u64 = 0;
        unsafe {
            COUNTER += 1;
            Self {
                id: COUNTER,
                created_at: Some(std::time::Instant::now()),
            }
        }
    }

    pub fn age(&self) -> Duration {
        self.created_at.map(|t| t.elapsed()).unwrap_or(Duration::ZERO)
    }
}

#[derive(Debug)]
pub struct ExampleManager {
    pub max_lifetime: Option<Duration>,
    pub connection_count: Arc<AtomicU64>,
}

impl ExampleManager {
    pub fn new(max_lifetime: Option<Duration>) -> Self {
        Self {
            max_lifetime,
            connection_count: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl Manager for ExampleManager {
    type Connection = ExampleConnection;
    type Error = String;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        self.connection_count.fetch_add(1, Ordering::SeqCst);
        Ok(ExampleConnection::new())
    }

    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        // 检查连接是否超过最大生命周期
        if let Some(max_lifetime) = self.max_lifetime {
            if conn.age() > max_lifetime {
                return Err(format!("Connection {} expired (age: {:?}, max: {:?})",
                                conn.id, conn.age(), max_lifetime));
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 使用 CheckDurationConnectionManager 管理连接生命周期 ===");

    // 方式1: 使用检查间隔模式 - 减少频繁检查的开销
    println!("\n1. 检查间隔模式（减少检查频率）:");
    let base_manager = ExampleManager::new(None);
    let interval_manager = CheckDurationConnectionManager::new(
        base_manager,
        Duration::from_secs(5) // 每5秒检查一次
    );
    let pool1 = Pool::new(interval_manager);

    println!("创建带有检查间隔的连接池");
    let conn1 = pool1.get().await?;
    println!("获取连接: ID = {}", conn1.id);
    drop(conn1);

    // 方式2: 使用生命周期管理 - 强制连接过期
    println!("\n2. 连接生命周期模式:");
    let lifetime_manager = ExampleManager::new(Some(Duration::from_millis(500))); // 500ms生命周期
    let pool2 = Pool::new(lifetime_manager);

    println!("创建带有生命周期管理的连接池");
    let conn2 = pool2.get().await?;
    println!("获取连接: ID = {}, 年龄 = {:?}", conn2.id, conn2.age());
    drop(conn2);

    // 等待连接过期
    tokio::time::sleep(Duration::from_millis(600)).await;

    // 尝试获取连接，之前的连接应该已经过期
    match pool2.get_timeout(Some(Duration::from_millis(100))).await {
        Ok(new_conn) => println!("获取到新连接: ID = {}", new_conn.id),
        Err(e) => println!("获取连接失败: {}", e),
    }

    // 方式3: 组合使用 - 检查间隔 + 生命周期管理
    println!("\n3. 组合模式:");
    let base_manager3 = ExampleManager::new(Some(Duration::from_millis(300)));
    let combined_manager = CheckDurationConnectionManager::new(
        base_manager3,
        Duration::from_millis(100) // 每100ms检查一次
    );
    let pool3 = Pool::new(combined_manager);

    println!("创建组合模式的连接池");

    // 创建多个连接并观察生命周期管理
    for i in 0..3 {
        let conn = pool3.get().await?;
        println!("连接 {}: ID = {}, 年龄 = {:?}", i+1, conn.id, conn.age());
        drop(conn);

        tokio::time::sleep(Duration::from_millis(150)).await;

        // 下次获取时可能会检测到连接过期
        match pool3.get_timeout(Some(Duration::from_millis(50))).await {
            Ok(new_conn) => println!("   获取到新连接: ID = {}", new_conn.id),
            Err(e) => println!("   连接过期被丢弃: {}", e),
        }
    }

    println!("\n=== 演示完成 ===");
    println!("总结:");
    println!("- set_max_idle_conns(): 在 Pool 层面实现，控制空闲连接数量");
    println!("- set_conn_max_lifetime(): 通过 CheckDurationConnectionManager 实现，控制连接生命周期");
    println!("- 两者功能互补，不冲突");

    Ok(())
}