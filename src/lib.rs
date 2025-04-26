#![allow(async_fn_in_trait)]

#[macro_use]
mod defer;

use flume::{Receiver, Sender};
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Pool have manager, get/get_timeout Connection from Pool
pub struct Pool<M: Manager> {
    manager: Arc<M>,
    idle_send: Arc<Sender<M::Connection>>,
    idle_recv: Arc<Receiver<M::Connection>>,
    max_open: Arc<AtomicU64>,
    in_use: Arc<AtomicU64>,
    waits: Arc<AtomicU64>,
}

impl<M: Manager> Debug for Pool<M> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pool")
            // .field("manager", &self.manager)
            .field("max_open", &self.max_open)
            .field("in_use", &self.in_use)
            .finish()
    }
}

impl<M: Manager> Clone for Pool<M> {
    fn clone(&self) -> Self {
        Self {
            manager: self.manager.clone(),
            idle_send: self.idle_send.clone(),
            idle_recv: self.idle_recv.clone(),
            max_open: self.max_open.clone(),
            in_use: self.in_use.clone(),
            waits: self.waits.clone(),
        }
    }
}

/// Manager create Connection and check Connection
pub trait Manager {
    type Connection;

    type Error: for<'a> From<&'a str>;

    ///create Connection and check Connection
    async fn connect(&self) -> Result<Self::Connection, Self::Error>;
    ///check Connection is alive? if not return Error(Connection will be drop)
    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error>;
}

impl<M: Manager> Pool<M> {
    pub fn new(m: M) -> Self
    where
        <M as Manager>::Connection: Unpin,
    {
        let default_max = num_cpus::get() as u64;
        let (s, r) = flume::unbounded();
        Self {
            manager: Arc::new(m),
            idle_send: Arc::new(s),
            idle_recv: Arc::new(r),
            max_open: Arc::new(AtomicU64::new(default_max)),
            in_use: Arc::new(AtomicU64::new(0)),
            waits: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn get(&self) -> Result<ConnectionBox<M>, M::Error> {
        self.get_timeout(None).await
    }

    pub async fn get_timeout(&self, d: Option<Duration>) -> Result<ConnectionBox<M>, M::Error> {
        self.waits.fetch_add(1, Ordering::SeqCst);
        defer!(|| {
            self.waits.fetch_sub(1, Ordering::SeqCst);
        });
        let f = async {
            loop {
                let connections = self.idle_send.len() as u64 + self.in_use.load(Ordering::SeqCst);
                if connections < self.max_open.load(Ordering::SeqCst) {
                    //Use In_use placeholder when create connection
                    self.in_use.fetch_add(1, Ordering::SeqCst);
                    defer!(||{
                        self.in_use.fetch_sub(1, Ordering::SeqCst);
                    });
                    //create connection,this can limit max idle,current now max idle = max_open
                    let conn = self.manager.connect().await?;
                    self.idle_send
                        .send(conn)
                        .map_err(|e| M::Error::from(&e.to_string()))?;
                }
                let mut conn = self
                    .idle_recv
                    .recv_async()
                    .await
                    .map_err(|e| M::Error::from(&e.to_string()))?;
                //check connection
                match self.manager.check(&mut conn).await {
                    Ok(_) => {
                        break Ok(conn);
                    }
                    Err(_e) => {
                        drop(conn);
                        if false {
                            return Err(_e);
                        }
                        continue;
                    }
                }
            }
        };
        let conn = {
            if d.is_none() {
                f.await?
            } else {
                tokio::time::timeout(d.unwrap(), f)
                    .await
                    .map_err(|_e| M::Error::from("get_timeout"))??
            }
        };
        Ok(ConnectionBox::new(
            conn,
            self.clone(),
        ))
    }

    pub fn state(&self) -> State {
        State {
            max_open: self.max_open.load(Ordering::Relaxed),
            connections: self.in_use.load(Ordering::Relaxed) + self.idle_send.len() as u64,
            in_use: self.in_use.load(Ordering::Relaxed),
            idle: self.idle_send.len() as u64,
            waits: self.waits.load(Ordering::Relaxed),
        }
    }

    pub fn set_max_open(&self, n: u64) {
        if n == 0 {
            return;
        }
        self.max_open.store(n, Ordering::SeqCst);
        loop {
            if self.idle_send.len() > n as usize {
                _ = self.idle_recv.try_recv();
            } else {
                break;
            }
        }
    }
}

pub struct ConnectionBox<M: Manager> {
    pub inner: Option<M::Connection>,
    pool: Pool<M>,
}

impl<M: Manager> Debug for ConnectionBox<M> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionBox")
            .field("pool", &self.pool)
            .finish()
    }
}

impl<M: Manager> Deref for ConnectionBox<M> {
    type Target = M::Connection;

    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().unwrap()
    }
}

impl<M: Manager> DerefMut for ConnectionBox<M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.as_mut().unwrap()
    }
}

impl<M: Manager> ConnectionBox<M> {
    pub fn new(
        conn: M::Connection,
        pool: Pool<M>,
    ) -> ConnectionBox<M> {
        pool.in_use.fetch_add(1, Ordering::SeqCst);
        Self {
            inner: Some(conn),
            pool,
        }
    }
}

impl<M: Manager> Drop for ConnectionBox<M> {
    fn drop(&mut self) {
        self.pool.in_use.fetch_sub(1, Ordering::SeqCst);
        if let Some(v) = self.inner.take() {
            let max_open = self.pool.max_open.load(Ordering::SeqCst);
            if self.pool.idle_send.len() as u64 + self.pool.in_use.load(Ordering::SeqCst) < max_open {
                _ = self.pool.idle_send.send(v);
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct State {
    /// max open limit
    pub max_open: u64,
    ///connections = in_use number + idle number
    pub connections: u64,
    /// user use connection number
    pub in_use: u64,
    /// idle connection
    pub idle: u64,
    /// wait get connections number
    pub waits: u64,
}

impl Display for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{ max_open: {}, connections: {}, in_use: {}, idle: {}, waits: {} }}",
            self.max_open, self.connections, self.in_use, self.idle, self.waits
        )
    }
}
