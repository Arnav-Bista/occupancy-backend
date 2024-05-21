use bytes::Bytes;
use chrono::{Date, DateTime, NaiveDate};
use chrono_tz::Tz;
use http_body_util::Full;
use hyper::{
    body::{Body, Incoming},
    service::Service,
    Request, Response, StatusCode, Uri,
};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, Statement};
use serde::Serialize;

use std::{collections::HashMap, error::Error, future::Future, pin::Pin, str::FromStr, sync::Arc};

#[derive(Clone)]
pub struct Server {
    connection_pool: Arc<Pool<SqliteConnectionManager>>,
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
    ) -> rusqlite::Result<Option<String>> {
        let mut statement = connection.prepare("SELECT time FROM gym ORDER BY id DESC LIMIT 1")?;
        let mut data = statement.query(())?;
        match data.next()? {
            Some(data) => {
                let data: String = data.get(0)?;
                Ok(Some(data.split_whitespace().next().unwrap().to_string()))
            }
            None => Ok(None),
        }
    }

    fn query_day(
        connection: &PooledConnection<SqliteConnectionManager>,
        date: &str,
    ) -> rusqlite::Result<Vec<(String, u16)>> {
        // SQL Injections are automatically handled by rusqlite
        let mut statement =
            connection.prepare(r"SELECT time,occupancy FROM gym WHERE time LIKE :date || '%'")?;
        let data = statement.query_map(&[(":date", &date)], |row| {
            let time: String = row.get(0)?;
            let occupancy: u16 = row.get(1)?;
            Ok((time, occupancy))
        })?;
        let mut results: Vec<(String, u16)> = Vec::new();
        for pair in data {
            match pair {
                Ok(pair) => results.push(pair),
                Err(_) => (),
            }
        }
        Ok(results)
    }

    fn day_data(&self, res: Request<Incoming>) -> Result<Response<Full<Bytes>>, hyper::Error> {
        // Not my proudest function 
        let connection = match self.get_connection() {
            Ok(conn) => conn,
            Err(err) => return Self::server_error(&err),
        };
        if let Some(params) = res.uri().query() {
            if let Some(map) = Self::parse_params(params) {
                if let Some(date) = map.get("date") {
                    if NaiveDate::from_str(date).is_err() {
                        return Self::bad_request("Malformed Date");
                    }
                    match Self::query_day(&connection, date) {
                        Ok(data) => return Self::ok_data(data),
                        Err(err) => return Self::server_error(&err.to_string()),
                    }
                }
            }
        }
        // Fetch the last recorded day's data instead
        match Self::get_last_day(&connection) {
            Err(err) => return Self::server_error(&err.to_string()),
            Ok(data) => match data {
                None => return Self::no_data(),
                Some(data) => match Self::query_day(&connection, &data) {
                    Ok(data) => return Self::ok_data(data),
                    Err(err) => return Self::server_error(&err.to_string()),
                },
            },
        }
    }

    fn ok_data<T: Serialize>(body: T) -> Result<Response<Full<Bytes>>, hyper::Error> {
        let data = serde_json::to_string(&body).unwrap();
        let res = Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
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

    fn not_found() -> Result<Response<Full<Bytes>>, hyper::Error> {
        let res = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::new()))
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
        let res = match req.uri().path() {
            "/api/gym/day" => self.day_data(req),
            _ => Server::not_found(),
        };

        Box::pin(async { res })
    }
}
