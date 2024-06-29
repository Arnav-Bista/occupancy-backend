use chrono::{DateTime, Datelike, Days, NaiveDate, NaiveDateTime, TimeDelta, Timelike};
use chrono_tz::Tz;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use reqwest::RequestBuilder;
use tokio::time::{sleep_until, Duration, Instant};

use core::panic;
use std::{collections::HashMap, f64, fs, path::Path, sync::Arc};

use crate::{
    database::sqlite::SqliteDatabase,
    predictor::knn_regressor::KNNRegressor,
    scraper::sta::main_library::MainLibrary,
    timing::{schedule::Schedule, uk_datetime_now::uk_datetime_now},
    ISO_FORMAT,
};

use super::sta::gym::Gym;

pub struct Scraper {
    connection_pool: Arc<Pool<SqliteConnectionManager>>,
    knn_config: HashMap<String, String>,
}

impl Scraper {
    pub fn setup(connection_pool: Arc<Pool<SqliteConnectionManager>>) -> Result<Self, String> {
        // Our hardcoded scrapers
        Self::create_table(&connection_pool, "gym")?;
        Self::create_table(&connection_pool, "main_library")?;
        let knn_config = Self::read_knn_config()?;

        Ok(Self {
            connection_pool,
            knn_config,
        })
    }

    fn read_knn_config() -> Result<HashMap<String, String>, String> {
        let mut map = HashMap::new();
        let path = Path::new("knn_config/");
        if !path.exists() {
            fs::create_dir(path).unwrap();
            return Ok(map);
        }

        for entry in path.read_dir().expect("Could not read knn_config.") {
            if let Ok(entry) = entry {
                let entry = entry.path();
                let name = path.file_name().unwrap().to_str().unwrap();
                let data = fs::read_to_string(entry).unwrap();
                map.insert(name.to_string(), data);
                return Ok(map);
            }
        }

        Ok(map)
    }

    fn update_knn_config(name: &str, data: &str) -> Result<(), String> {
        let path = Path::new("knn_config/").join(name);
        match fs::write(path, data) {
            Ok(_) => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    }

    pub async fn run(self) {
        let gym = Gym::new(self.knn_config.get("gym").cloned());
        let library = MainLibrary::new(self.knn_config.get("main_library").cloned());
        println!("Running!");
        tokio::spawn(Self::run_scraper(self.connection_pool.clone(), gym));
        tokio::spawn(Self::run_scraper(self.connection_pool.clone(), library));
    }

    async fn run_scraper<T: Scrape<T>>(
        connection_pool: Arc<Pool<SqliteConnectionManager>>,
        mut target: T,
    ) {
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

            let occupancy = occupancy.unwrap();
            let schedule = schedule.unwrap();

            let connection = match connection_pool.get() {
                Ok(conn) => conn,
                Err(_) => {
                    println!("Could not get database connection - Scrape.");
                    return;
                }
            };

            if schedule.is_open(timestamp) {
                match SqliteDatabase::insert_one_occupancy(
                    &connection,
                    &T::table_name(),
                    timestamp.naive_local(),
                    occupancy,
                ) {
                    Err(err) => println!("Error writing to database.\n{}", err.to_string()),
                    _ => (),
                };
            }

            Self::check_and_predict(&mut target, &connection_pool, &schedule);

            Self::standard_sleep().await;
        }
    }

    async fn standard_sleep() {
        sleep_until(Instant::now() + Duration::from_secs(30 * 10)).await;
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
        let table_name = name.to_string() + "_schedule";
        match connection.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (
                    id INTEGER PRIMARY KEY,
                    date TEXT NOT NULL,
                    schedule NOT NULL
                )",
                table_name
            ),
            (),
        ) {
            Err(_) => return Err(format!("Could not create table '{}'.", name).to_string()),
            _ => (),
        };
        let table_name = name.to_string() + "_prediction_knn";
        match connection.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (
                    id INTEGER PRIMARY KEY,
                    time TEXT NOT NULL,
                    occupancy INTEGER NOT NULL
                )",
                table_name
            ),
            (),
        ) {
            Err(_) => return Err(format!("Could not create table '{}'.", name).to_string()),
            _ => (),
        };
        let table_name = name.to_string() + "_prediction_lstm";
        match connection.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (
                    id INTEGER PRIMARY KEY,
                    time TEXT NOT NULL,
                    occupancy INTEGER NOT NULL
                )",
                table_name
            ),
            (),
        ) {
            Err(_) => return Err(format!("Could not create table '{}'.", name).to_string()),
            _ => (),
        };
        Ok(())
    }

    fn check_and_predict<T: Scrape<T>>(
        target: &mut T,
        connection_pool: &Arc<Pool<SqliteConnectionManager>>,
        schedule: &Schedule,
    ) {
        let today = uk_datetime_now().naive_local().date();
        let next_week = today.checked_add_days(Days::new(7)).unwrap();
        let last_updated = target.get_last_updated();

        match last_updated {
            Some(last_updated) => {
                if last_updated <= next_week {
                    // Already up to date with the predictions, nothing to do.
                    return;
                }
                // last_updated is less than next_week
                Self::make_knn_predictions(
                    target,
                    connection_pool,
                    last_updated,
                    next_week,
                    schedule,
                );
            }
            None => {
                // Assume data is not there.
                Self::make_knn_predictions(target, connection_pool, today, next_week, schedule)
            }
        }
    }

    fn get_last_n_weeks_data_grouped<T: Scrape<T>>(
        target: &T,
        connection_pool: &Arc<Pool<SqliteConnectionManager>>,
        n: usize,
    ) -> Result<Vec<Vec<(NaiveDateTime, u16)>>, String> {
        let to = uk_datetime_now().naive_local();
        let from = to.checked_sub_days(Days::new(n as u64 * 7)).unwrap();

        let connection = match connection_pool.get() {
            Ok(connection) => connection,
            Err(err) => return Err("Could not get connection.".to_string()),
        };
        let table_name = &T::table_name();
        let data = match SqliteDatabase::query_range(&connection, &table_name, from, to) {
            Ok(data) => data,
            Err(err) => return Err(err.to_string()),
        };


        let data: Vec<(NaiveDateTime, u16)> = data
            .iter()
            .map(|(time, occu)| {
                let time = NaiveDateTime::parse_from_str(time, ISO_FORMAT).unwrap();
                (time, *occu)
            })
            .collect();

        let mut grouped_data: Vec<Vec<(NaiveDateTime, u16)>> = vec![Vec::new(); 7];
        for element in data {
            let day = element.0.weekday().number_from_monday() - 1;
            grouped_data[day as usize].push(element);
        }
        Ok(grouped_data)
    }

    fn make_knn_predictions<T: Scrape<T>>(
        target: &mut T,
        connection_pool: &Arc<Pool<SqliteConnectionManager>>,
        from: NaiveDate,
        to: NaiveDate,
        schedule: &Schedule,
    ) {
        let data = match Self::get_last_n_weeks_data_grouped(target, connection_pool, 3) {
            Ok(data) => data,
            Err(err) => {
                println!("Could not get data for KNN predictions.\n{}", err);
                return;
            }
        };

        let mut final_predictions: Vec<(NaiveDateTime, u16)> = Vec::new();

        let timings = schedule.get_timings();
        let mut current_date = from;
        while current_date <= to {
            // To keep track of the DateTime for later
            // Could probably be avoided by iterating through original vec tho
            let mut original_time: Vec<NaiveDateTime> = Vec::new();
            // Construct the data
            let mut x: Vec<(f64, f64)> = Vec::new();
            let mut y: Vec<f64> = Vec::new();
            let index = (current_date.weekday().number_from_monday() - 1) as usize;

            // Default if closed
            let opening_hm = timings[index].opening().unwrap_or(630) as u32;
            let closing_hm = timings[index].closing().unwrap_or(2230) as u32;

            // HM should not be invalid!
            // If so, something went wrong in the scraper or database
            let opening = current_date
                .and_hms_opt(opening_hm / 100 as u32, opening_hm % 100, 0)
                .unwrap();
            let closing = current_date
                .and_hms_opt(closing_hm / 100 as u32, closing_hm % 100, 0)
                .unwrap();

            for (time, occupancy) in &data[index] {
                let weight: f64 = 1.0 / ((opening - *time).num_weeks() + 1) as f64;
                original_time.push(*time);
                let time = time.num_seconds_from_midnight() as f64;
                let occupancy = *occupancy as f64;
                x.push((weight, time));
                y.push(occupancy);
            }

            let predictions = KNNRegressor::predict_range(
                x,
                y,
                opening.num_seconds_from_midnight() as f64,
                closing.num_seconds_from_midnight() as f64,
                5.0 * 60.0,
                3,
            );

            // Convert timestamp back to NaiveDateTime
            for ((_, occupancy), original) in predictions.iter().zip(original_time.iter()) {
                let occupancy = *occupancy as u16;
                final_predictions.push((*original, occupancy));
            }
            current_date = current_date.checked_add_days(Days::new(1)).unwrap();
        }
        let connection = match connection_pool.get() {
            Ok(connection) => connection,
            Err(err) => {
                println!("Could not get connection for KNN predictions.\n{}", err);
                return;
            }
        };
        match SqliteDatabase::insert_many_occupancy(
            &connection,
            &format!("{}{}", T::table_name(), "_prediction_knn"),
            final_predictions,
        ) {
            Err(err) => println!("Could not insert KNN predictions.\n{}", err),
            _ => (),
        };

        // Update the last updated time
        target.set_last_updated(to);
        match Self::update_knn_config(&T::table_name(), &to.to_string()) {
            Ok(_) => (),
            Err(err) => println!("Could not update KNN config.\n{}", err),
        };

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

    fn get_last_updated(&self) -> Option<NaiveDate>;

    fn set_last_updated(&mut self, last_updated: NaiveDate);
}
