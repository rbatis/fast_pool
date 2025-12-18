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

pub trait QPS {
    fn qps(&self, total: u64);
    fn time(&self, total: u64);
    fn cost(&self);
}

impl QPS for std::time::Instant {
    fn qps(&self, total: u64) {
        let time = self.elapsed();
        println!(
            "QPS: {} QPS/s",
            (total as u128 * 1000000000 as u128 / time.as_nanos() as u128)
        );
    }

    fn time(&self, total: u64) {
        let time = self.elapsed();
        println!(
            "Time: {:?} ,each:{} ns/op",
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

pub fn block_on<T, R>(task: T) -> R
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

// 运行命令: cargo bench --package fast_pool --performance
// 或者: cargo bench
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
        tokio::task::block_in_place(|| {
            rt.block_on(async {
                let _conn = pool.get().await.unwrap();
            });
        });
    });
}
