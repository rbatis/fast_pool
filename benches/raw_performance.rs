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
use test::Bencher;

pub trait QPS {
    fn qps(&self, total: u64);
    fn time(&self, total: u64);
    fn cost(&self);
}

impl QPS for std::time::Instant {
    fn qps(&self, total: u64) {
        let time = self.elapsed();
        println!(
            "use QPS: {} QPS/s",
            (total as u128 * 1000000000 as u128 / time.as_nanos() as u128)
        );
    }

    fn time(&self, total: u64) {
        let time = self.elapsed();
        println!(
            "use Time: {:?} ,each:{} ns/op",
            &time,
            time.as_nanos() / (total as u128)
        );
    }

    fn cost(&self) {
        let time = self.elapsed();
        println!("cost:{:?}", time);
    }
}

#[macro_export]
macro_rules! rbench {
    ($total:expr,$body:block) => {{
        let now = std::time::Instant::now();
        for _ in 0..$total {
            $body;
        }
        now.time($total);
        now.qps($total);
    }};
}

pub fn block_on<T,R>(task: T)-> R
    where
        T: Future<Output = R> + Send + 'static,
        T::Output: Send + 'static,
{
    tokio::task::block_in_place(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio block_on fail")
            .block_on(task)
    })
}


//cargo test --release --package fast_pool --bench raw_performance bench_pool --no-fail-fast --  --exact -Z unstable-options --show-output
//windows:
//---- bench_pool stdout ----
//use Time: 4.0313ms ,each:40 ns/op
//use QPS: 24749412 QPS/s
//macos:
//---- bench_pool stdout ----
// use Time: 6.373708ms ,each:63 ns/op
// use QPS: 15683710 QPS/s
#[test]
fn bench_pool() {
    use async_trait::async_trait;
    use fast_pool::{Pool, Manager};

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
    let f = async {
        let p = Pool::new(TestManager {});
        rbench!(100000, {
              let v = p.get().await.unwrap();
        });
    };
    block_on(f);
}


