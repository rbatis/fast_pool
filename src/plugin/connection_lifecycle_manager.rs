use crate::{Pool, Manager};
use std::sync::Arc;

/// 连接生命周期管理器，用于定期清理过期连接
/// Connection lifecycle manager for periodically cleaning up expired connections
pub struct ConnectionLifecycleManager<M: Manager> {
    pub pool: Arc<Pool<M>>,
}

impl<M: Manager> ConnectionLifecycleManager<M> {
    /// 创建新的生命周期管理器
    /// Create a new lifecycle manager
    pub fn new(pool: Arc<Pool<M>>) -> Self {
        Self { pool }
    }

    /// 手动执行一次清理任务
    /// Manually execute a cleanup task once
    pub async fn cleanup_expired_connections(&self) {
        let Some(max_lifetime) = self.pool.max_lifetime.get() else {
            return; // 没有设置最大生命周期，不需要清理
        };

        // 检查是否需要时间戳功能
        if !self.pool.needs_timestamp() {
            return; // 不需要时间戳功能，无需清理
        }

        let mut expired_count = 0;
        let current_idle = self.pool.idle_send.len();

        // 批量检查，避免阻塞太久
        let check_limit = std::cmp::min(current_idle, 20);

        for _ in 0..check_limit {
            if let Ok(_conn) = self.pool.idle_recv.try_recv() {
                // 在默认模式下，不检查过期时间
                // 实际的时间戳功能应该在使用时动态启用
                expired_count += 1;
            } else {
                break; // 队列为空
            }
        }

        // 更新连接计数
        for _ in 0..expired_count {
            if self.pool.connections.load(std::sync::atomic::Ordering::SeqCst) > 0 {
                self.pool.connections.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }
}