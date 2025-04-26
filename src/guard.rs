use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::Ordering;
use crate::{Manager, Pool};

/// ConnectionGuard is a wrapper for Connection
pub struct ConnectionGuard<M: Manager> {
    pub inner: Option<M::Connection>,
    pool: Pool<M>,
}

impl<M: Manager> ConnectionGuard<M> {
    pub fn new(conn: M::Connection, pool: Pool<M>) -> ConnectionGuard<M> {
        pool.in_use.fetch_add(1, Ordering::SeqCst);
        Self {
            inner: Some(conn),
            pool,
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
    fn drop(&mut self) {
        if let Some(v) = self.inner.take() {
            _ = self.pool.recycle(v);
        }
    }
}

