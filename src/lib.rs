use async_trait::async_trait;
use flume::{Receiver, Sender};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Pool have manager, get/get_timeout Connection from Pool
#[derive(Debug)]
pub struct Pool<M: Manager> {
    manager: Arc<M>,
    idle_send: Sender<M::Connection>,
    idle_recv: Receiver<M::Connection>,
    max_open: Arc<AtomicU64>,
    in_use: Arc<AtomicU64>,
}

impl<M: Manager> Clone for Pool<M> {
    fn clone(&self) -> Self {
        Self {
            manager: self.manager.clone(),
            idle_send: self.idle_send.clone(),
            idle_recv: self.idle_recv.clone(),
            max_open: self.max_open.clone(),
            in_use: self.in_use.clone(),
        }
    }
}

/// Manager create Connection and check Connection
#[async_trait]
pub trait Manager {
    type Connection;

    type Error: for<'a> From<&'a str>;

    ///create Connection and check Connection
    async fn connect(&self) -> Result<Self::Connection, Self::Error>;
    ///check Connection is alive? if not return Error(Connection will be drop)
    async fn check(&self, conn: Self::Connection) -> Result<Self::Connection, Self::Error>;
}

impl<M: Manager> Pool<M> {
    pub fn new(m: M) -> Self {
        let default_max = num_cpus::get() as u64 * 4;
        let (s, r) = flume::unbounded();
        Self {
            manager: Arc::new(m),
            idle_send: s,
            idle_recv: r,
            max_open: Arc::new(AtomicU64::new(default_max)),
            in_use: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn get(&self) -> Result<ConnectionBox<M>, M::Error> {
        self.get_timeout(None).await
    }

    pub async fn get_timeout(&self, d: Option<Duration>) -> Result<ConnectionBox<M>, M::Error> {
        //pop connection from channel
        let f = async {
            let connections = self.in_use.load(Ordering::SeqCst) + self.idle_send.len() as u64;
            if connections < self.max_open.load(Ordering::SeqCst) {
                let conn = self.manager.connect().await?;
                self.idle_send
                    .send(conn)
                    .map_err(|e| M::Error::from(&e.to_string()))?;
            }
            self.idle_recv
                .recv_async()
                .await
                .map_err(|e| M::Error::from(&e.to_string()))
        };
        let mut conn = {
            if d.is_none() {
                f.await?
            } else {
                tokio::time::timeout(d.unwrap(), f)
                    .await
                    .map_err(|_e| M::Error::from("get_timeout"))??
            }
        };
        //check connection
        match self.manager.check(conn).await {
            Ok(v) => {
                conn = v;
                self.in_use.fetch_add(1, Ordering::SeqCst);
            }
            Err(e) => {
                return Err(e);
            }
        }
        Ok(ConnectionBox {
            inner: Some(conn),
            sender: self.idle_send.clone(),
            in_use: self.in_use.clone(),
            max_open: self.max_open.clone(),
        })
    }

    pub fn state(&self) -> State {
        State {
            max_open: self.max_open.load(Ordering::Relaxed),
            connections: self.in_use.load(Ordering::Relaxed) + self.idle_send.len() as u64,
            in_use: self.in_use.load(Ordering::Relaxed),
            idle: self.idle_send.len() as u64,
        }
    }

    pub fn set_max_open(&self, n: u64) {
        self.max_open.store(n, Ordering::SeqCst);
        let open = self.idle_send.len() as u64;
        if open > n {
            let del = open - n;
            for _ in 0..del {
                _ = self.idle_recv.try_recv();
            }
        }
    }
}

#[derive(Debug)]
pub struct ConnectionBox<M: Manager> {
    pub inner: Option<M::Connection>,
    sender: Sender<M::Connection>,
    in_use: Arc<AtomicU64>,
    max_open: Arc<AtomicU64>,
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

impl<M: Manager> Drop for ConnectionBox<M> {
    fn drop(&mut self) {
        self.in_use.fetch_sub(1, Ordering::SeqCst);
        if let Some(v) = self.inner.take() {
            let max_open = self.max_open.load(Ordering::SeqCst);
            if self.sender.len() as u64 + self.in_use.load(Ordering::SeqCst) < max_open {
                _ = self.sender.send(v);
            }
        }
    }
}

#[derive(Debug)]
pub struct State {
    /// max open limit
    pub max_open: u64,
    ///connections = in_use number + idle number
    pub connections: u64,
    /// user use connection number
    pub in_use: u64,
    /// idle connection
    pub idle: u64,
}
