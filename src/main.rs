extern crate core;
use redis_rust_server_2::lib::run;

#[tokio::main]
async fn main() {
    run().await;
}

