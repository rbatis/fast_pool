# fast_pool

[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)
[![GitHub release](https://img.shields.io/github/v/release/rbatis/fast_pool)](https://github.com/rbatis/fast_pool/releases)

<img style="width: 200px;height: 190px;" width="200" height="190" src="https://github.com/rbatis/rbatis/raw/master/logo.png" />

a fast async pool based on channel
* support `get()`,`get_timeout()`,`state()` methods
* support atomic max_open(Resize freely)
* based on [flume](https://crates.io/crates/flume)

### way fast_pool?

* fast get() method performance
```log
//windows:
//---- bench_pool stdout ----
//Time: 14.0994ms ,each:140 ns/op
//QPS: 7086167 QPS/s
//macos:
//---- bench_pool stdout ----
//Time: 6.373708ms ,each:63 ns/op
//QPS: 15683710 QPS/s
```


### how to use this?

* step 1 add toml
```toml
fast_pool="0.3"
tokio = {version = "1",features = ["time","rt-multi-thread","macros"]}
```
* step 2 impl trait
```rust
use std::ops::{DerefMut};
use std::time::Duration;
use async_trait::async_trait;
use fast_pool::{Manager, Pool};

#[derive(Debug)]
pub struct TestManager {}

impl Manager for TestManager {
    type Connection = String;
    type Error = String;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        Ok("conn".to_string())
    }

    async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        //check should use conn.ping()
        if conn == "error" {
            return Err(Self::Error::from("error".to_string()));
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let p = Pool::new(TestManager {});
    println!("status = {}",p.state());
    p.set_max_open(10);
    println!("status = {}",p.state());

    let mut conn = p.get().await.unwrap();
    println!("conn = {}",conn.deref_mut());
    let mut conn = p.get_timeout(Some(Duration::from_secs(1))).await.unwrap();
    println!("conn = {}",conn.deref_mut());
}
```
