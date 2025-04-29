use crate::guard::ConnectionGuard;
use crate::state::State;
use crate::Manager;
use flume::{Receiver, Sender};
use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use dark_std::sync::AtomicDuration;

/// Pool have manager, get/get_timeout Connection from Pool
pub struct Pool<M: Manager> {
    pub manager: Arc<M>,
    pub idle_send: Arc<Sender<M::Connection>>,
    pub idle_recv: Arc<Receiver<M::Connection>>,
    pub max_open: Arc<AtomicU64>,
    pub(crate) in_use: Arc<AtomicU64>,
    pub(crate) waits: Arc<AtomicU64>,
    pub(crate) connecting: Arc<AtomicU64>,
    pub(crate) checking: Arc<AtomicU64>,
    pub(crate) connections: Arc<AtomicU64>,
    //timeout check connection default 10s
    pub timeout_check: Arc<AtomicDuration>,
}

impl<M: Manager> Debug for Pool<M> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let state = self.state();
        Debug::fmt(&state, f)
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
            timeout_check: self.timeout_check.clone(),
        }
    }
}

impl<M: Manager> Pool<M> {
    pub fn new(m: M) -> Self
    where
        M::Connection: Unpin,
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
            timeout_check: Arc::new(AtomicDuration::new(Some(Duration::from_secs(10)))),
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
            let v: Result<ConnectionGuard<M>, M::Error> = loop {
                let connections = self.connections.load(Ordering::SeqCst)
                    + self.connecting.load(Ordering::SeqCst);
                if connections < self.max_open.load(Ordering::SeqCst) {
                    //Use In_use placeholder when create connection
                    self.connecting.fetch_add(1, Ordering::SeqCst);
                    defer!(|| {
                        self.connecting.fetch_sub(1, Ordering::SeqCst);
                    });
                    //create connection,this can limit max idle,current now max idle = max_open
                    let conn = self.manager.connect().await?;
                    self.idle_send
                        .send(conn)
                        .map_err(|e| M::Error::from(&e.to_string()))?;
                    self.connections.fetch_add(1, Ordering::SeqCst);
                }
                let conn = self
                    .idle_recv
                    .recv_async()
                    .await
                    .map_err(|e| M::Error::from(&e.to_string()))?;
                let mut guard = ConnectionGuard::new(conn, self.clone());
                guard.set_checked(false);
                //check connection
                self.checking.fetch_add(1, Ordering::SeqCst);
                defer!(|| {
                    self.checking.fetch_sub(1, Ordering::SeqCst);
                });
                let check_result = tokio::time::timeout(
                    self.timeout_check.get().unwrap_or_default(),
                    self.manager.check(&mut guard),
                )
                .await
                .map_err(|e| M::Error::from(&format!("check_timeout={}", e)))?;
                match check_result {
                    Ok(_) => {
                        guard.set_checked(true);
                        break Ok(guard);
                    }
                    Err(_e) => {
                        drop(guard);
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
        Ok(conn)
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

    pub fn get_max_open(&self) -> u64 {
        self.max_open.load(Ordering::SeqCst)
    }

    pub fn recycle(&self, arg: M::Connection) {
        self.in_use.fetch_sub(1, Ordering::SeqCst);
        if self.idle_send.len() < self.max_open.load(Ordering::SeqCst) as usize {
            _ = self.idle_send.send(arg);
        } else {
            if self.connections.load(Ordering::SeqCst) > 0 {
                self.connections.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    /// Set the timeout for checking connections in the pool.
    pub fn set_timeout_check(&self, duration: Option<Duration>) {
        self.timeout_check.store(duration);
    }

    /// Set the timeout for checking connections in the pool.
    pub fn get_timeout_check(&self) -> Option<Duration> {
        self.timeout_check.get()
    }
}
