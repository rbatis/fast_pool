use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use async_trait::async_trait;
use flume::{Receiver, Sender};

pub struct Pool<M: Manager> {
    manager: M,
    sender: Sender<M::Connection>,
    receiver: Receiver<M::Connection>,
    max_open: Arc<AtomicU64>,
    in_use: Arc<AtomicU64>,
}

#[async_trait]
pub trait Manager {
    type Connection: Send + 'static;

    type Error: for<'a> From<&'a str> + ToString + Send + Sync + 'static;

    async fn connect(&self) -> Result<Self::Connection, Self::Error>;
    async fn check(&self, conn: Self::Connection) -> Result<Self::Connection, Self::Error>;
}

impl<M: Manager> Pool<M> {
    pub fn new(m: M) -> Self {
        let (s, r) = flume::unbounded();
        Self {
            manager: m,
            sender: s,
            receiver: r,
            max_open: Arc::new(AtomicU64::new(10)),
            in_use: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn get(&self) -> Result<ConnectionBox<M>, M::Error> {
        self.get_timeout(None).await
    }

    pub async fn get_timeout(&self, d: Option<Duration>) -> Result<ConnectionBox<M>, M::Error> {
        //pop connection from channel
        let f = async {
            if self.in_use.load(Ordering::SeqCst) <= self.max_open.load(Ordering::SeqCst) {
                let conn = self.manager.connect().await?;
                self.sender.send(conn).map_err(|e| M::Error::from(&e.to_string()))?;
            }
            self.receiver.recv_async().await.map_err(|e| M::Error::from(&e.to_string()))
        };
        let mut conn = {
            if d.is_none() {
                f.await?
            } else {
                tokio::time::timeout(d.unwrap(), f).await.map_err(|_e| M::Error::from("get timeout"))??
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
            sender: self.sender.clone(),
            in_use: self.in_use.clone(),
        })
    }

    pub async fn state(&self) -> State {
        State {
            max_open: self.max_open.load(Ordering::Relaxed),
            connections: self.sender.len() as u64,
            in_use: self.in_use.load(Ordering::Relaxed),
        }
    }

    pub async fn set_max_open(&self, n: u64) {
        let open = self.sender.len() as u64;
        if open > n {
            let del = open - n;
            for _ in 0..del {
                _ = self.receiver.try_recv();
            }
        }
        self.max_open.store(n, Ordering::SeqCst);
    }
}

pub struct ConnectionBox<M: Manager> {
    inner: Option<M::Connection>,
    sender: Sender<M::Connection>,
    in_use: Arc<AtomicU64>,
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
        if let Some(v) = self.inner.take() {
            _ = self.sender.send(v);
        }
        self.in_use.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct State {
    pub max_open: u64,
    pub connections: u64,
    pub in_use: u64,
}

#[cfg(test)]
mod test {
    use std::ops::Deref;
    use std::time::Duration;
    use async_trait::async_trait;
    use crate::{Pool, Manager};

    pub struct TestManager {}

    #[async_trait]
    impl Manager for TestManager {
        type Connection = i32;
        type Error = String;

        async fn connect(&self) -> Result<Self::Connection, Self::Error> {
            Ok(0)
        }

        async fn check(&self, conn: Self::Connection) -> Result<Self::Connection, Self::Error> {
            Ok(conn)
        }
    }

    // --nocapture
    #[tokio::test]
    async fn test_pool_get() {
        let p = Pool::new(TestManager {});
        let mut arr = vec![];
        for i in 0..10 {
            let v = p.get().await.unwrap();
            println!("{},{}", i, v.inner.unwrap());
            arr.push(v);
        }
    }

    #[tokio::test]
    async fn test_pool_get_timeout() {
        let p = Pool::new(TestManager {});
        p.set_max_open(10);
        let mut arr = vec![];
        for i in 0..10 {
            let v = p.get().await.unwrap();
            println!("{},{}", i, v.deref());
            arr.push(v);
        }
        assert_eq!(p.get_timeout(Some(Duration::from_secs(0))).await.is_err(), true);
    }
}