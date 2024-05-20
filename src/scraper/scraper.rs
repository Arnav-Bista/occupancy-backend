use chrono::DateTime;
use chrono_tz::Tz;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use reqwest::RequestBuilder;
use tokio::time::{sleep_until, Duration, Instant};

use std::sync::Arc;

use crate::{scraper::sta::main_library::MainLibrary, timing::{schedule::Schedule, uk_datetime_now::uk_datetime_now}};

use super::sta::gym::Gym;

pub struct Scraper {
    connection_pool: Arc<Pool<SqliteConnectionManager>>,
}

impl Scraper {
    pub fn setup(connection_pool: Arc<Pool<SqliteConnectionManager>>) -> Result<Self, String> {
        // Our hardcoded scrapers
        Self::create_table(&connection_pool, "gym")?;
        Self::create_table(&connection_pool, "main_library")?;
        Ok(Self { connection_pool })
    }

    pub async fn run(self) {
        println!("Running!");
        let gym = Gym::new();
        let library = MainLibrary::new();
        // tokio::spawn(Self::run_scraper(self.connection_pool.clone(), gym));
        tokio::spawn(Self::run_scraper(self.connection_pool.clone(), library));
    }

    async fn run_scraper<T: Scrape<T>>(connection_pool: Arc<Pool<SqliteConnectionManager>>, target: T) {
        loop {
            let (occupancy, schedule, timestamp) = match target.scrape(target.get_request()).await {
                Err(err) => {
                    println!("{}", err);
                    Self::standard_sleep().await;
                    continue;
                }
                Ok(data) => data,
            };
            if occupancy.is_none() || schedule.is_none() {
                Self::standard_sleep().await;
                continue;
            }

            if schedule.unwrap().is_open(timestamp) {
                Self::write_to_database(&connection_pool, occupancy.unwrap(), timestamp, &T::table_name());
            }
            Self::standard_sleep().await;
        }
    }

    async fn standard_sleep() {
        sleep_until(Instant::now() + Duration::from_secs(30 * 10)).await;
    }

    fn write_to_database(connection_pool: &Arc<Pool<SqliteConnectionManager>>, occupancy: u16, timestamp: DateTime<Tz>, name: &str) {
        let connection = match connection_pool.get() {
            Ok(conn) => conn,
            Err(_) => {
                println!("Could not get database connection");
                return;
            }
        };

        let result = connection.execute(
            &format!("INSERT INTO {} (time, occupancy) VALUES (?1, ?2)", &name),
            (&timestamp.to_string(), &occupancy),
        );
        match result {
            Err(err) => println!("Error writing to database.\n{}",err.to_string()),
            _ => (),
        }
    }

    fn create_table(
        connection_pool: &Arc<Pool<SqliteConnectionManager>>,
        name: &str,
    ) -> Result<(), String> {
        let connection = match connection_pool.get() {
            Ok(connection) => connection,
            Err(_) => {
                return Err("Couldn't obtain a connection for database setup - Scraper.".to_owned())
            }
        };
        match connection.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (
                    id INTEGER PRIMARY KEY,
                    time TEXT NOT NULL,
                    occupancy INTEGER NOT NULL
                )",
                name
            ),
            (),
        ) {
            Err(_) => return Err(format!("Could not create table '{}'.", name).to_string()),
            _ => (),
        };
        Ok(())
    }
}

pub trait Scrape<T> {
    fn table_name() -> String;

    fn get_request(&self) -> RequestBuilder;

    async fn scrape(
        &self,
        request: RequestBuilder,
    ) -> Result<(Option<u16>, Option<Schedule>, DateTime<Tz>), String> {
        let response = match request.send().await {
            Ok(data) => data,
            Err(err) => return Err(err.to_string()),
        };

        let body = match response.text().await {
            Ok(text) => text,
            Err(err) => return Err(err.to_string()),
        };
        let timestamp = uk_datetime_now();
        Ok((
            Self::parse_occupancy(&self, &body),
            Self::parse_schedule(&self, &body),
            timestamp,
        ))
    }

    fn parse_occupancy(&self, body: &str) -> Option<u16>;
    fn parse_schedule(&self, body: &str) -> Option<Schedule>;
}
