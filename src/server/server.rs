use bytes::Bytes;
use chrono::NaiveDate;
use http_body_util::Full;
use hyper::{body::Incoming, service::Service, Method, Request, Response, StatusCode};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use regex::Regex;
use serde::Serialize;

use std::{collections::HashMap, future::Future, pin::Pin, str::FromStr, sync::Arc};

use crate::timing::schedule::{self, Schedule};

#[derive(Clone)]
pub struct Server {
    connection_pool: Arc<Pool<SqliteConnectionManager>>,
}

#[derive(Serialize)]
struct MyResponse {
    data: Vec<(String, u16)>,
    schedule: String,
}

impl MyResponse {
    pub fn new(data: Vec<(String, u16)>, schedule: String) -> Self {
        Self { data, schedule }
    }
}

impl Server {
    pub fn setup(connection_pool: Arc<Pool<SqliteConnectionManager>>) -> Self {
        Self { connection_pool }
    }

    fn parse_params(text: &str) -> Option<HashMap<String, String>> {
        let mut map: HashMap<String, String> = HashMap::new();
        for pairs in text.split('&').into_iter() {
            let mut iterator = pairs.split('=').into_iter();
            map.insert(iterator.next()?.to_string(), iterator.next()?.to_string());
        }
        Some(map)
    }

    fn get_connection(&self) -> Result<PooledConnection<SqliteConnectionManager>, String> {
        match self.connection_pool.get() {
            Err(err) => {
                return Err(format!(
                    "Could not get connection - Server.\n{}",
                    err.to_string()
                ));
            }
            Ok(conn) => Ok(conn),
        }
    }

    fn get_last_day(
        connection: &PooledConnection<SqliteConnectionManager>,
        name: &str,
    ) -> rusqlite::Result<Option<String>> {
        // Name should already be sanitized!
        let mut statement = connection.prepare(&format!(
            "SELECT time FROM {} ORDER BY id DESC LIMIT 1",
            name
        ))?;
        let mut data = statement.query(())?;
        match data.next()? {
            Some(data) => {
                let data: String = data.get(0)?;
                Ok(Some(data.split_whitespace().next().unwrap().to_string()))
            }
            None => Ok(None),
        }
    }

    fn query_schedule(
        connection: &PooledConnection<SqliteConnectionManager>,
        date: &str,
        name: &str,
    ) -> rusqlite::Result<Option<String>> {
        // SQL Injections are automatically handled by rusqlite
        // Name should already be sanitized!
        let mut statement = connection.prepare(&format!(
            "SELECT schedule FROM {}_schedule WHERE date LIKE ?1",
            name
        ))?;

        let mut data = statement.query(rusqlite::params![date])?;
        match data.next()? {
            Some(data) => {
                let data: String = data.get(0)?;
                Ok(Some(data))
            }
            None => Ok(None),
        }
    }

    fn query_day(
        connection: &PooledConnection<SqliteConnectionManager>,
        date: &str,
        name: &str,
    ) -> Result<Response<Full<Bytes>>, hyper::Error> {
        // SQL Injections are automatically handled by rusqlite
        // Name should already be sanitized!
        let mut statement = match connection.prepare(&format!(
            "SELECT time,occupancy FROM {} WHERE time LIKE ?1 || '%'",
            name
        )) {
            Ok(statement) => statement,
            Err(err) => {
                let message = err.to_string();
                if message.contains("no such table") {
                    return Self::not_found(&message);
                }
                return Self::server_error(&err.to_string());
            }
        };
        let data = match statement.query_map(rusqlite::params![date], |row| {
            let time: String = row.get(0)?;
            let occupancy: u16 = row.get(1)?;
            Ok((time, occupancy))
        }) {
            Ok(data) => data,
            Err(err) => return Self::server_error(&err.to_string()),
        };
        let mut occupancy_data: Vec<(String, u16)> = Vec::new();
        for pair in data {
            match pair {
                Ok(pair) => occupancy_data.push(pair),
                Err(_) => (),
            }
        }

        let schedule = match Self::query_schedule(connection, date, name) {
            Ok(schedule) => match schedule {
                None => return Self::no_data(),
                Some(schedule) => schedule,
            },
            Err(err) => return Self::server_error(&err.to_string()),
        };

        let result = MyResponse::new(occupancy_data, schedule);
        Self::ok_data(result)
    }

    fn day_data(&self, res: Request<Incoming>) -> Result<Response<Full<Bytes>>, hyper::Error> {
        // Not my proudest function
        let connection = match self.get_connection() {
            Ok(conn) => conn,
            Err(err) => return Self::server_error(&err),
        };

        let Some(params) = res.uri().query() else {
            return Self::bad_request("Parameters not provided. Required name + Optional date.");
        };

        let Some(map) = Self::parse_params(params) else {
            return Self::bad_request("Malformed Parameters.");
        };

        let Some(name) = map.get("name") else {
            return Self::bad_request("name not provided.");
        };

        // SQL Injections are automatically handled by rusqlite
        // Handle the table name manually
        let name = match Regex::new(r"(\w+)").unwrap().captures(name) {
            None => return Self::bad_request("Malformed Name"),
            Some(captures) => captures,
        };
        let name = name.get(0).unwrap().as_str();
        if name.is_empty() {
            return Self::bad_request("Malformed Name");
        }

        if let Some(date) = map.get("date") {
            if NaiveDate::from_str(date).is_err() {
                return Self::bad_request("Malformed Date");
            }
            return Self::query_day(&connection, date, &name);
        }
        // Fetch the last recorded day's data instead
        match Self::get_last_day(&connection, &name) {
            Err(err) => return Self::server_error(&err.to_string()),
            Ok(data) => match data {
                None => return Self::no_data(),
                Some(data) => return Self::query_day(&connection, &data, &name),
            },
        };
    }

    fn ok_data<T: Serialize>(body: T) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let data = serde_json::to_string(&body).unwrap();
        let res = Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from(format!("{{\"data\": {}}}", data))))
            .unwrap();
        Ok(res)
    }

    fn server_error(message: &str) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let res = Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Full::new(Bytes::from(format!(
                "{{\"error\": \"{}\" }}",
                message
            ))))
            .unwrap();
        Ok(res)
    }

    fn not_found(message: &str) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let res = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(if message.is_empty() {
                Bytes::new()
            } else {
                Bytes::from(format!("{{\"error\": \"{}\" }}", message))
            }))
            .unwrap();
        Ok(res)
    }

    fn bad_request(message: &str) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let res = Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Full::new(Bytes::from(format!(
                "{{\"error\": \"{}\" }}",
                message
            ))))
            .unwrap();
        Ok(res)
    }

    fn no_data() -> Result<Response<Full<Bytes>>, hyper::Error> {
        let res = Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Full::new(Bytes::new()))
            .unwrap();
        Ok(res)
    }
}

impl Service<Request<Incoming>> for Server {
    type Response = Response<Full<Bytes>>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let res = match req.method() {
            &Method::GET => match req.uri().path() {
                "/api/day" => self.day_data(req),
                _ => Server::not_found(""),
            },
            _ => Server::not_found(""),
        };

        Box::pin(async { res })
    }
}
