use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

use std::sync::Arc;

pub struct Server {
    connection_pool: Arc<Pool<SqliteConnectionManager>>
}

impl Server {
    pub fn setup(connection_pool: Arc<Pool<SqliteConnectionManager>>) -> Self {
        Self {
            connection_pool
        }
    }
}
