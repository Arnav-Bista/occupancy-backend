use bytes::Bytes;
use chrono::{NaiveDate, NaiveDateTime};
use http_body_util::Full;
use hyper::{body::Incoming, service::Service, Method, Request, Response, StatusCode};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use regex::Regex;
use serde::Serialize;
use url_escape::decode;

use std::{collections::HashMap, future::Future, pin::Pin, str::FromStr, sync::Arc};

use crate::database::sqlite::SqliteDatabase;

use super::myresponse::MyResponse;

#[derive(Clone)]
pub struct Server {
    connection_pool: Arc<Pool<SqliteConnectionManager>>,
    name_sanitizer: Regex,
}

impl Server {
    pub fn setup(connection_pool: Arc<Pool<SqliteConnectionManager>>) -> Self {
        Self {
            connection_pool,
            name_sanitizer: Regex::new(r"(\w+)").unwrap(),
        }
    }

    fn parse_params(text: &str) -> Option<HashMap<String, String>> {
        let mut map: HashMap<String, String> = HashMap::new();
        for pairs in text.split('&').into_iter() {
            let mut iterator = pairs.split('=').into_iter();
            map.insert(
                iterator.next()?.to_string(),
                decode(iterator.next()?).to_string(),
            );
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

    fn get_single_day(
        connection: &PooledConnection<SqliteConnectionManager>,
        date: NaiveDate,
        name: &str,
    ) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let data: Vec<(String, u16)> =
            match SqliteDatabase::query_single_day(connection, name, date) {
                Ok(data) => data,
                Err(err) => match err {
                    rusqlite::Error::QueryReturnedNoRows => return Self::no_data(),
                    _ => return Self::server_error(&err.to_string()),
                },
            };
        let knn_prediction: Vec<(String, u16)> = match SqliteDatabase::query_single_day(
            connection,
            &format!("{}{}", name, "_prediction_knn"),
            date,
        ) {
            Ok(data) => data,
            Err(err) => match err {
                rusqlite::Error::QueryReturnedNoRows => return Self::no_data(),
                _ => return Self::server_error(&err.to_string()),
            },
        };
        let schedule = match SqliteDatabase::query_single_day_schedule(connection, name, date) {
            Ok(schedule) => match schedule {
                None => return Self::no_data(),
                Some(schedule) => schedule,
            },
            Err(err) => return Self::server_error(&err.to_string()),
        };

        let result = MyResponse::new(data, serde_json::from_str(&schedule).unwrap(), knn_prediction);
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
        let name = match self.name_sanitizer.captures(name) {
            None => return Self::bad_request("Malformed Name"),
            Some(captures) => captures,
        };
        let name = name.get(0).unwrap().as_str();
        if name.is_empty() {
            return Self::bad_request("Malformed Name");
        }

        if let Some(date) = map.get("date") {
            if let Ok(date) = NaiveDate::from_str(date) {
                return Self::get_single_day(&connection, date, &name);
            }
            return Self::bad_request("Malformed Date");
        }
        // Fetch the last recorded day's data instead

        match SqliteDatabase::query_last_day(&connection, &name) {
            Err(err) => return Self::server_error(&err.to_string()),
            Ok(data) => match data {
                None => return Self::no_data(),
                Some(data) => match NaiveDate::from_str(&data) {
                    Err(_) => return Self::server_error("Could not parse date"),
                    Ok(date) => {
                        return Self::get_single_day(&connection, date, &name);
                    }
                },
            },
        };
    }
    fn query_from(
        connection: &PooledConnection<SqliteConnectionManager>,
        from: NaiveDateTime,
        name: &str,
    ) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let to = from + chrono::Duration::days(1);

        let occupancy_data = match SqliteDatabase::query_range(connection, name, from, to) {
            Ok(data) => data,
            Err(err) => match err {
                rusqlite::Error::QueryReturnedNoRows => return Self::no_data(),
                _ => return Self::server_error(&err.to_string()),
            },
        };

        let schedule =
            match SqliteDatabase::query_single_day_schedule(connection, name, from.date()) {
                Ok(schedule) => match schedule {
                    None => return Self::no_data(),
                    Some(schedule) => schedule,
                },
                Err(err) => return Self::server_error(&err.to_string()),
            };

        let result = MyResponse::new(
            occupancy_data,
            serde_json::from_str(&schedule).unwrap(),
            Vec::new(),
        );
        Self::ok_data(result)
    }

    fn from_last(&self, res: Request<Incoming>) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let connection = match self.get_connection() {
            Ok(conn) => conn,
            Err(err) => return Self::server_error(&err),
        };

        let Some(params) = res.uri().query() else {
            return Self::bad_request("Parameters not provided. Required name + Required from.");
        };

        let Some(map) = Self::parse_params(params) else {
            return Self::bad_request("Malformed Parameters.");
        };

        let Some(name) = map.get("name") else {
            return Self::bad_request("name not provided.");
        };

        let Some(from) = map.get("from") else {
            return Self::bad_request("from not provided.");
        };

        let name = match self.name_sanitizer.captures(name) {
            None => return Self::bad_request("Malformed Name"),
            Some(captures) => captures,
        };
        let name = name.get(0).unwrap().as_str();
        if name.is_empty() {
            return Self::bad_request("Malformed Name");
        }
        let from: NaiveDateTime = match NaiveDateTime::from_str(from) {
            Ok(date) => date,
            Err(_) => return Self::bad_request("Malformed Date"),
        };
        Self::query_from(&connection, from, name)
    }

    fn ok_data<T: Serialize>(body: T) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let data = serde_json::to_string(&body).unwrap();
        let res = Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from(data)))
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
                "/api/from" => self.from_last(req),
                _ => Server::not_found(""),
            },
            _ => Server::not_found(""),
        };

        Box::pin(async { res })
    }
}
