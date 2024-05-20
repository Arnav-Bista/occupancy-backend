mod scraper;
mod server;
mod timing;

use std::{error::Error, path::Path, sync::Arc};

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use scraper::scraper::Scraper;
use server::server::Server;
use tokio::net::{TcpListener, TcpStream};



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
        let (socket, address) = listener.accept().await.unwrap();
        tokio::spawn(async move {
            process(socket).await;
        });
    }
}

async fn process(socket: TcpStream) {

}
