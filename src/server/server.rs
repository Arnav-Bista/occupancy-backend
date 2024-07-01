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

use crate::{database::sqlite::SqliteDatabase, timing::schedule::Schedule};

use super::myresponse::MyResponse;

/// The Server
///
/// This is THE struct that handles all API endpoints and the business logic.
/// The actual querying part is handled by functions from `SqliteDatabase`.
///
/// This struct implements the `Service` trait from `hyper` which allows it to be used as a
/// hyper service. This allows us to send responses to requests from the client.
///
/// For each TCP connection or Client, a new thread is assigned to handle that request. We have to
/// clone this struct for each thread to make it thread save and avoid race conditions.

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

    /// Parses the query parameters and returns a `hashmap` of key pair values
    /// Returns `None` if the parameters are malformed
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

    /// Obtain a connection from the connection pool.
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

    /// Fetches the data for a single day.
    /// This is the /api/day API endpoint.
    ///
    /// Takes in a `connection` to query the database
    /// `date` to fetch the data for
    /// `name` of the table to fetch the data from
    ///
    /// Will return a 200 as long as there is at least a prediction for that day.
    /// If there is no Schedule data, the last recorded Schedule will be returned.
    ///
    /// Will return a 204 when there is no data and no prediction.
    fn get_single_day(
        connection: &PooledConnection<SqliteConnectionManager>,
        date: NaiveDate,
        name: &str,
    ) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let data: Vec<(String, u16)> =
            match SqliteDatabase::query_single_day(connection, name, date) {
                Ok(data) => data,
                Err(err) => match err {
                    rusqlite::Error::QueryReturnedNoRows => Vec::new(),
                    _ => return Self::server_error(&err.to_string()),
                },
            };
        // If there is no prediction at all, return a 204, otherwise proceed
        let knn_prediction: Vec<(String, u16)> = match SqliteDatabase::query_single_day(
            connection,
            &format!("{}{}", name, "_prediction_knn"),
            date,
        ) {
            Ok(data) => data,
            Err(err) => match err {
                rusqlite::Error::QueryReturnedNoRows => {
                    if data.is_empty() {
                        return Self::no_data();
                    }
                    Vec::new()
                }
                _ => return Self::server_error(&err.to_string()),
            },
        };
        let lstm_prediction: Vec<(String, u16)> = match SqliteDatabase::query_single_day(
            connection,
            &format!("{}{}", name, "_prediction_lstm"),
            date,
        ) {
            Ok(data) => data,
            Err(err) => match err {
                rusqlite::Error::QueryReturnedNoRows => {
                    if data.is_empty() {
                        return Self::no_data();
                    }
                    Vec::new()
                }
                _ => return Self::server_error(&err.to_string()),
            },
        };
        // Default to the last scraped Schedule if there is no schedule for the day
        let schedule: Schedule =
            match SqliteDatabase::query_single_day_schedule(connection, name, date) {
                Ok(schedule) => match schedule {
                    None => match SqliteDatabase::query_last_day_schedule(connection, name) {
                        Ok(schedule) => match schedule {
                            None => return Self::no_data(),
                            Some(schedule) => schedule,
                        },
                        Err(err) => return Self::server_error(&err.to_string()),
                    },
                    Some(schedule) => serde_json::from_str(&schedule).unwrap(),
                },
                Err(err) => return Self::server_error(&err.to_string()),
            };

        let result = MyResponse::new(data, schedule, knn_prediction, lstm_prediction);
        Self::ok_data(result)
    }

    /// The /api/day API endpoint.
    ///
    /// This handles all the URL preprocessing before actually calling the function. Avoids
    /// problems with SQL injections and handles invalid parameters beforehand.
    ///
    /// At the end, it calls the `get_single_day` function to fetch the data if a date is provided,
    /// otherwise gets the last recorded day's data using `query_last_day` into `get_single_day`.
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

    /// Fetches the data from a specific time onwards till the end of the day or the data that's
    /// collected so far.
    ///
    /// It uses the `query_range` function to fetch the data and the `query_single_day_schedule`
    /// for the schedule.
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
            Vec::new(),
        );
        Self::ok_data(result)
    }

    /// The /api/from API endpoint.
    ///
    /// This is the endpoint the frontend should use when it already has some data for the day.
    /// It will take in a datetime and return the rest of the data collected for that day.
    /// Again, this handles all the preprocessing, the actual data fetching is done by `query_from`.
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

    /// Return a 200 OK response with the data provided.
    fn ok_data<T: Serialize>(body: T) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let data = serde_json::to_string(&body).unwrap();
        let res = Response::builder()
            .status(StatusCode::OK)
            .body(Full::new(Bytes::from(data)))
            .unwrap();
        Ok(res)
    }

    /// Return a 500 Internal Server Error response with the message provided.
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

    /// Return a 404 Not Found response with the message provided. The message here is optional.
    /// Leave it empty for no message.
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

    /// Return a 400 Bad Request response with the message provided.
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

    /// Return a 204 No Content response.
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
