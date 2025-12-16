use crate::Manager;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 高性能的连接包装器，通过 enable_timestamp 字段控制是否跟踪时间戳
/// High-performance connection wrapper that controls timestamp tracking via enable_timestamp field
#[derive(Debug)]
pub struct TimeConnection<M: Manager> {
    pub connection: M::Connection,
    pub enable_timestamp: bool,  // 是否启用时间戳跟踪
    created_at_nanos: Option<u64>,  // 只有 enable_timestamp=true 时才使用
}

impl<M: Manager> TimeConnection<M> {
    /// 创建新连接，可选择是否记录时间戳
    pub fn new(connection: M::Connection, enable_timestamp: bool) -> Self {
        let created_at_nanos = if enable_timestamp {
            Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as u64,
            )
        } else {
            None
        };

        Self {
            connection,
            enable_timestamp,
            created_at_nanos,
        }
    }

    /// 检查连接是否超过了指定的最大生命周期
    #[inline]
    pub fn is_expired(&self, max_lifetime: Duration) -> bool {
        if !self.enable_timestamp {
            return false; // 未启用时间戳的连接永不过期
        }

        let Some(created_at_nanos) = self.created_at_nanos else {
            return false;
        };

        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        // 检查是否超过最大生命周期
        let age_nanos = now_nanos.saturating_sub(created_at_nanos);
        age_nanos > max_lifetime.as_nanos() as u64
    }

    /// 获取连接的年龄
    #[inline]
    pub fn age(&self) -> Duration {
        if !self.enable_timestamp {
            return Duration::ZERO;
        }

        let Some(created_at_nanos) = self.created_at_nanos else {
            return Duration::ZERO;
        };

        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let age_nanos = now_nanos.saturating_sub(created_at_nanos);
        Duration::from_nanos(age_nanos)
    }

    /// 检查是否启用了时间戳跟踪
    #[inline]
    pub fn has_timestamp(&self) -> bool {
        self.enable_timestamp
    }

    /// 消费连接并返回内部的连接对象
    #[inline]
    pub fn into_connection(self) -> M::Connection {
        self.connection
    }
}