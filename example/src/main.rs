use fast_pool::{Manager, Pool};
use std::ops::DerefMut;
use std::time::Duration;

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
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let p = Pool::new(TestManager {});
    println!("state = {}", p.state());
    p.set_max_open(10);
    println!("state = {}", p.state());

    let mut conn = p.get().await?;
    println!("conn = {}", conn.deref_mut());
    let mut conn = p.get_timeout(Some(Duration::from_secs(1))).await?;
    println!("conn = {}", conn.deref_mut());
    Ok(())
}
