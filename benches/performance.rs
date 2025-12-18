#![allow(unused_mut)]
#![allow(unused_imports)]
#![allow(unreachable_patterns)]
#![allow(unused_variables)]
#![allow(unused_assignments)]
#![allow(unused_must_use)]
#![allow(dead_code)]
#![feature(test)]
extern crate test;

use futures_core::future::BoxFuture;
use std::any::Any;
use std::future::Future;
use std::time::Duration;
use test::Bencher;

// cargo bench bench_pool
#[bench]
fn bench_pool(b: &mut Bencher) {
    use async_trait::async_trait;
    use fast_pool::{Manager, Pool};

    pub struct TestManager {}

    impl Manager for TestManager {
        type Connection = i32;
        type Error = String;

        async fn connect(&self) -> Result<Self::Connection, Self::Error> {
            Ok(0)
        }

        async fn check(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    // 使用tokio runtime来运行异步代码
    let rt = tokio::runtime::Runtime::new().unwrap();
    let pool = rt.block_on(async {
        Pool::new(TestManager {})
    });

    // 直接在benchmark中使用block_in_place，避免channel开销
    b.iter(|| {
       rt.block_on(async {
         let _conn = pool.get().await.unwrap();
       });
    });
}
