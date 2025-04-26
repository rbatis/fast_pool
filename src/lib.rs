#![allow(async_fn_in_trait)]

#[macro_use]
mod defer;
mod state;
mod guard;

use flume::{Receiver, Sender};
use std::fmt::{Debug, Formatter};
use std::ops::{DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use crate::guard::ConnectionGuard;
use crate::state::State;

/// Manager create Connection and check Connection
pub trait Manager {
    type Connection;

    type Error: for<'a> From<&'a str>;

    ///create Connection and check Connection
    async fn connect(&self) -> Result<Self::Connection, Self::Error>;
    ///check Connection is alive? if not return Error(Connection will be drop)
    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error>;
}

/// Pool have manager, get/get_timeout Connection from Pool
pub struct Pool<M: Manager> {
    manager: Arc<M>,
    pub idle_send: Arc<Sender<M::Connection>>,
    idle_recv: Arc<Receiver<M::Connection>>,
    max_open: Arc<AtomicU64>,
    in_use: Arc<AtomicU64>,
    waits: Arc<AtomicU64>,
    connecting: Arc<AtomicU64>,
    checking: Arc<AtomicU64>,
    connections: Arc<AtomicU64>,
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
            connecting: self.connecting.clone(),
            checking: self.checking.clone(),
            connections: self.connections.clone(),
        }
    }
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
            connecting: Arc::new(AtomicU64::new(0)),
            checking: Arc::new(AtomicU64::new(0)),
            connections: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn get(&self) -> Result<ConnectionGuard<M>, M::Error> {
        self.get_timeout(None).await
    }

    pub async fn get_timeout(&self, d: Option<Duration>) -> Result<ConnectionGuard<M>, M::Error> {
        self.waits.fetch_add(1, Ordering::SeqCst);
        defer!(|| {
            self.waits.fetch_sub(1, Ordering::SeqCst);
        });
        let f = async {
            let v: Result<M::Connection, M::Error> = loop {
                let connections = self.connections.load(Ordering::SeqCst);
                if connections < self.max_open.load(Ordering::SeqCst) {
                    self.connections.fetch_add(1, Ordering::SeqCst);
                    //Use In_use placeholder when create connection
                    self.connecting.fetch_add(1, Ordering::SeqCst);
                    defer!(|| {
                        self.connecting.fetch_sub(1, Ordering::SeqCst);
                    });
                    //create connection,this can limit max idle,current now max idle = max_open
                    let conn = self.manager.connect().await;
                    if conn.is_err() {
                        self.connections.fetch_sub(1, Ordering::SeqCst);
                    }
                    let v = self
                        .idle_send
                        .send(conn?)
                        .map_err(|e| M::Error::from(&e.to_string()));
                    if v.is_err() {
                        self.connections.fetch_sub(1, Ordering::SeqCst);
                    }
                    v?
                }
                let mut conn = self
                    .idle_recv
                    .recv_async()
                    .await
                    .map_err(|e| M::Error::from(&e.to_string()))?;
                //check connection
                self.checking.fetch_add(1, Ordering::SeqCst);
                defer!(|| {
                    self.checking.fetch_sub(1, Ordering::SeqCst);
                });
                match self.manager.check(&mut conn).await {
                    Ok(_) => {
                        break Ok(conn);
                    }
                    Err(_e) => {
                        drop(conn);
                        self.connections.fetch_sub(1, Ordering::SeqCst);
                        continue;
                    }
                }
            };
            v
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
        Ok(ConnectionGuard::new(conn, self.clone()))
    }

    pub fn state(&self) -> State {
        State {
            max_open: self.max_open.load(Ordering::Relaxed),
            connections: self.connections.load(Ordering::Relaxed),
            in_use: self.in_use.load(Ordering::SeqCst),
            idle: self.idle_send.len() as u64,
            waits: self.waits.load(Ordering::SeqCst),
            connecting: self.connecting.load(Ordering::SeqCst),
            checking: self.checking.load(Ordering::SeqCst),
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
                if self.connections.load(Ordering::SeqCst) > 0 {
                    self.connections.fetch_sub(1, Ordering::SeqCst);
                }
            } else {
                break;
            }
        }
    }

    pub fn recycle(&self, arg: M::Connection) {
        self.in_use.fetch_sub(1, Ordering::SeqCst);
        if self.idle_send.len() < self.max_open.load(Ordering::SeqCst) as usize {
            _ = self.idle_send.send(arg);
        } else {
            self.connections.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

