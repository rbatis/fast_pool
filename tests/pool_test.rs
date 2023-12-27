use fast_pool::{Manager, Pool};

#[cfg(test)]
mod test {
    use crate::{Manager, Pool};
    use async_trait::async_trait;
    use std::ops::Deref;
    use std::time::Duration;

    #[derive(Debug)]
    pub struct TestManager {}

    #[async_trait]
    impl Manager for TestManager {
        type Connection = String;
        type Error = String;

        async fn connect(&self) -> Result<Self::Connection, Self::Error> {
            Ok(String::new())
        }

        async fn check(&self, conn: Self::Connection) -> Result<Self::Connection, Self::Error> {
            if conn == "error" {
                return Err(Self::Error::from(&conn));
            }
            Ok(conn)
        }
    }

    #[tokio::test]
    async fn test_debug() {
        let p = Pool::new(TestManager {});
        println!("{:?}", p);
    }

    // --nocapture
    #[tokio::test]
    async fn test_pool_get() {
        let p = Pool::new(TestManager {});
        p.set_max_open(10);
        let mut arr = vec![];
        for i in 0..10 {
            let v = p.get().await.unwrap();
            println!("{},{}", i, v.deref());
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
        assert_eq!(
            p.get_timeout(Some(Duration::from_secs(0))).await.is_err(),
            true
        );
    }

    #[tokio::test]
    async fn test_pool_check() {
        let p = Pool::new(TestManager {});
        p.set_max_open(10);
        let mut v = p.get().await.unwrap();
        *v.inner.as_mut().unwrap() = "error".to_string();
        for _i in 0..10 {
            let v = p.get().await.unwrap();
            assert_eq!(v.deref() == "error", false);
        }
    }

    #[tokio::test]
    async fn test_pool_resize() {
        let p = Pool::new(TestManager {});
        p.set_max_open(10);
        let mut arr = vec![];
        for i in 0..10 {
            let v = p.get().await.unwrap();
            println!("{},{}", i, v.deref());
            arr.push(v);
        }
        assert_eq!(
            p.get_timeout(Some(Duration::from_secs(0))).await.is_err(),
            true
        );
        p.set_max_open(11);
        assert_eq!(
            p.get_timeout(Some(Duration::from_secs(0))).await.is_err(),
            false
        );
        arr.push(p.get().await.unwrap());
        assert_eq!(
            p.get_timeout(Some(Duration::from_secs(0))).await.is_err(),
            true
        );
    }

    #[tokio::test]
    async fn test_pool_resize2() {
        let p = Pool::new(TestManager {});
        p.set_max_open(2);
        let mut arr = vec![];
        for _i in 0..2 {
            let v = p.get().await.unwrap();
            arr.push(v);
        }
        p.set_max_open(1);
        drop(arr);
        println!("{:?}", p.state());
        assert_eq!(
            p.get_timeout(Some(Duration::from_secs(0))).await.is_err(),
            false
        );
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let p = Pool::new(TestManager {});
        p.set_max_open(10);
        let mut handles = vec![];
        for _ in 0..10 {
            let pool = p.clone();
            let handle = tokio::spawn(async move {
                let _ = pool.get().await.unwrap();
            });
            handles.push(handle);
        }
        for handle in handles {
            handle.await.unwrap();
        }
        assert_eq!(p.state().connections, 10);
    }

    #[tokio::test]
    async fn test_invalid_connection() {
        let p = Pool::new(TestManager {});
        p.set_max_open(10);

        let mut conn = p.get().await.unwrap();
        *conn.inner.as_mut().unwrap() = "error".to_string();

        // Attempt to get a new connection, should not be the invalid one
        let new_conn = p.get().await.unwrap();
        assert_ne!(new_conn.deref(), &"error".to_string());
    }

    #[tokio::test]
    async fn test_connection_lifetime() {
        let p = Pool::new(TestManager {});
        p.set_max_open(10);

        let conn = p.get().await.unwrap();
        // Perform operations using the connection
        // ...

        drop(conn); // Drop the connection

        // Ensure dropped connection is not in use
        assert_eq!(p.state().in_use, 0);

        // Acquire a new connection
        let new_conn = p.get().await.unwrap();
        assert_ne!(new_conn.deref(), &"error".to_string());
    }

    #[tokio::test]
    async fn test_boundary_conditions() {
        let p = Pool::new(TestManager {});
        p.set_max_open(2);

        // Acquire connections until pool is full
        let conn_1 = p.get().await.unwrap();
        let _conn_2 = p.get().await.unwrap();
        assert_eq!(p.state().in_use, 2);

        // Attempt to acquire another connection (pool is full)
        assert!(p.get_timeout(Some(Duration::from_secs(0))).await.is_err());

        // Release one connection, pool is no longer full
        drop(conn_1);
        assert_eq!(p.state().in_use, 1);

        // Acquire another connection (pool has space)
        let _conn_3 = p.get().await.unwrap();
        assert_eq!(p.state().in_use, 2);

        // Increase pool size
        p.set_max_open(3);
        // Acquire another connection after increasing pool size
        let _conn_4 = p.get().await.unwrap();
        assert_eq!(p.state().in_use, 3);
    }
}
