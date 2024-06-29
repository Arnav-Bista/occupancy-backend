mod scraper;
mod server;
mod timing;
mod predictor;
mod database;

use std::sync::Arc;

use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use r2d2_sqlite::SqliteConnectionManager;
use scraper::scraper::Scraper;
use server::server::Server;
use tokio::net::TcpListener;

pub const ISO_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
pub const ISO_FORMAT_DATE: &str = "%Y-%m-%d";

#[tokio::main]
async fn main() {
    let manager = SqliteConnectionManager::file("data.db");
    let pool = r2d2::Pool::builder().build(manager).unwrap();
    let pool = Arc::new(pool);

    let scraper = Scraper::setup(pool.clone()).unwrap();
    let server = Server::setup(pool.clone());

    tokio::spawn(async move {
        scraper.run().await;
    });

    let listener = TcpListener::bind("127.0.0.1:7878").await.unwrap();

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        let server_clone = server.clone();
        tokio::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, server_clone)
                .await
            {
                println!("{}", err);
            }
        });
    }
}
