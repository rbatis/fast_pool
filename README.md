# fast_pool
a fast async pool based on channel
* support `get()`,`get_timeout()`,`state()` methods
* support atomic max_open(Resize freely)
* based on [flume channel](https://crates.io/crates/flume)

### way fast_pool?

* fast get() method performance
```log
//windows:
//---- bench_pool stdout ----
//use Time: 4.0313ms ,each:40 ns/op
//use QPS: 24749412 QPS/s
//macos:
//---- bench_pool stdout ----
// use Time: 6.373708ms ,each:63 ns/op
// use QPS: 15683710 QPS/s
```


### how to use this?

* add toml
```toml
fast_pool="0.1"
async-trait = "0.1"
tokio = {version = "1",features = ["time","rt-multi-thread","macros"]}
```
* impl trait
```rust
    use std::time::Duration;
    use async_trait::async_trait;
    use crate::{Pool, Manager};

    pub struct TestManager {}

    pub struct MyConnection {}

    #[async_trait]
    impl Manager for TestManager {
        type Connection = MyConnection;
        type Error = String;

        async fn connect(&self) -> Result<Self::Connection, Self::Error> {
            //connect get an MyConnection
            Ok(MyConnection{})
        }

        async fn check(&self, conn: Self::Connection) -> Result<Self::Connection, Self::Error> {
            //use conn.ping() check connection
            Ok(conn)
        }
    }

    #[tokio::main]
    async fn main() {
        let pool = Pool::new(TestManager {});
        pool.set_max_open(10);
        for i in 0..10 {
            let v = pool.get().await.unwrap();
            println!("{},{}", i, v.deref());
        }
    }

```