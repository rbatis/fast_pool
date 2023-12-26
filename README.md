# fast_pool
a fast async pool based on channel
* support `get()`,`get_timeout()`,`state()` methods
* support atomic max_open(Resize freely)
* support atomic in_use(Resize freely)

### way fast_pool?

* fast performance
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
```
* impl trait
```rust
    use std::time::Duration;
    use async_trait::async_trait;
    use crate::{ChannelPool, RBPoolManager};

    pub struct TestManager {}

    #[async_trait]
    impl RBPoolManager for TestManager {
        type Connection = i32;
        type Error = String;

        async fn connect(&self) -> Result<Self::Connection, Self::Error> {
            Ok(0)
        }

        async fn check(&self, conn: Self::Connection) -> Result<Self::Connection, Self::Error> {
            Ok(conn)
        }
    }

    #[tokio::main]
    async fn main() {
        let p = ChannelPool::new(TestManager {});
        for i in 0..10 {
            let v = p.get().await.unwrap();
            println!("{},{}", i, v.inner.unwrap());
        }
    }

```