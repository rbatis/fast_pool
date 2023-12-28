use async_trait::async_trait;
use fast_pool::{Manager, Pool};
use std::ops::DerefMut;
use std::time::Duration;

#[derive(Debug)]
pub struct TestManager {}

#[async_trait]
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
    println!("status = {:?}", p.state());
    p.set_max_open(10);
    println!("status = {:?}", p.state());

    let mut conn = p.get().await.unwrap();
    println!("conn = {}", conn.deref_mut());
    let mut conn = p.get_timeout(Some(Duration::from_secs(1))).await.unwrap();
    println!("conn = {}", conn.deref_mut());
}
