use crate::{Manager, Pool};
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::Ordering;

/// RAII guard that automatically returns connection to pool on drop
pub struct ConnectionGuard<M: Manager> {
    pub inner: Option<M::Connection>,
    pool: Pool<M>,
    checked: bool,
}

impl<M: Manager> ConnectionGuard<M> {
    /// Create new connection guard
    pub fn new(conn: M::Connection, pool: Pool<M>) -> ConnectionGuard<M> {
        Self {
            inner: Some(conn),
            pool,
            checked: false,
        }
    }

    /// Mark connection as checked and update pool stats
    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
        if checked {
            self.pool.in_use.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl<M: Manager> Debug for ConnectionGuard<M> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionGuard")
            .field("pool", &self.pool)
            .finish()
    }
}

impl<M: Manager> Deref for ConnectionGuard<M> {
    type Target = M::Connection;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().unwrap()
    }
}

impl<M: Manager> DerefMut for ConnectionGuard<M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.as_mut().unwrap()
    }
}

impl<M: Manager> Drop for ConnectionGuard<M> {
    /// Return connection to pool or cleanup failed connection
    fn drop(&mut self) {
        if self.checked == false {
            // Failed connection - decrement connection count
            if self.pool.connections.load(Ordering::SeqCst) > 0 {
                self.pool.connections.fetch_sub(1, Ordering::SeqCst);
            }
        } else {
            // Valid connection - return to pool
            if let Some(v) = self.inner.take() {
                _ = self.pool.recycle(v);
            }
        }
    }
}
