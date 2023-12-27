# fast_pool

[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)
[![GitHub release](https://img.shields.io/github/v/release/rbatis/fast_pool)](https://github.com/rbatis/fast_pool/releases)

<img style="width: 200px;height: 200px;" width="200" height="200" src="https://github.com/rbatis/rbatis/raw/master/logo.png" />

a fast async pool based on channel
* support `get()`,`get_timeout()`,`state()` methods
* support atomic max_open(Resize freely)
* based on [crossbeam channel(crossfire)](https://crates.io/crates/crossfire)

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
use std::ops::{DerefMut};
use std::time::Duration;
use async_trait::async_trait;
use fast_pool::{Manager, Pool};

#[derive(Debug)]
pub struct TestManager {}

#[async_trait]
impl Manager for TestManager {
    type Connection = String;
    type Error = String;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        Ok("conn".to_string())
    }

    async fn check(&self, conn: Self::Connection) -> Result<Self::Connection, Self::Error> {
        //check should use conn.ping()
        if conn == "error" {
            return Err(Self::Error::from(&conn));
        }
        Ok(conn)
    }
}

#[tokio::main]
async fn main() {
    let p = Pool::new(TestManager {});
    println!("status = {:?}",p.state());
    p.set_max_open(10);
    println!("status = {:?}",p.state());

    let mut conn = p.get().await.unwrap();
    println!("conn = {}",conn.deref_mut());
    let mut conn = p.get_timeout(Some(Duration::from_secs(1))).await.unwrap();
    println!("conn = {}",conn.deref_mut());
}
```