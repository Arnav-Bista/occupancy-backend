use std::u32;

use chrono::{NaiveDate, NaiveDateTime};
use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;

use crate::{timing::schedule::Schedule, ISO_FORMAT};

pub struct SqliteDatabase {}

impl SqliteDatabase {
    /**
    Get the most recent date in the database.

    Returns an `Ok(Some(String))` if successful.
    Returns an `Ok(None)` if the table is empty.
    */
    pub fn query_last_day(
        connection: &PooledConnection<SqliteConnectionManager>,
        table_name: &str,
    ) -> rusqlite::Result<Option<String>> {
        // Name should already be sanitized!
        let mut statement = connection.prepare(&format!(
            "SELECT time FROM {} ORDER BY time DESC LIMIT 1",
            table_name
        ))?;
        let mut data = statement.query(())?;
        match data.next()? {
            Some(data) => {
                let data: String = data.get(0)?;
                let data = NaiveDateTime::parse_from_str(&data, ISO_FORMAT).unwrap();
                Ok(Some(data.date().to_string()))
            }
            None => Ok(None),
        }
    }

    /**
    Get the occupancy for a single day.
    
    Uses the LIKE operator to get all rows that start with the date.
    */
    pub fn query_single_day(
        connection: &PooledConnection<SqliteConnectionManager>,
        table_name: &str,
        date: NaiveDate,
    ) -> rusqlite::Result<Vec<(String, u16)>> {
        // SQL Injections are automatically handled by rusqlite
        // Name should already be sanitized!
        let mut statement = connection.prepare(&format!(
            "SELECT time,occupancy FROM {} WHERE time LIKE ?1 || '%'",
            table_name
        ))?;

        let mut data: Vec<(String, u16)> = Vec::new();
        let rows = statement.query_map(
            rusqlite::params![date.to_string()],
            |row| {
                let time: String = row.get(0)?;
                let occupancy: u16 = row.get(1)?;
                Ok((time, occupancy))
            }
        )?;
        
        for row in rows {
            data.push(row?);
        }

        Ok(data)
    }

    
    /**
    Get the schedule for a single day.

    Returns an `Ok(Some(String))` if successful.
    Returns an `Ok(None)` if data is not found for that date.

    Parsing of Schedule by serde is left out for better error handling.
    */
    pub fn query_single_day_schedule(
        connection: &PooledConnection<SqliteConnectionManager>,
        table_name: &str,
        date: NaiveDate,
    ) -> rusqlite::Result<Option<String>> {
    
        let mut statement = connection.prepare(&format!(
            "SELECT schedule FROM {}_schedule WHERE date LIKE ?1",
            table_name
        ))?;

        let mut data = statement.query(rusqlite::params![date.to_string()])?;
        match data.next()? {
            Some(data) => {
                let data: String = data.get(0)?;
                Ok(Some(data))
            }
            None => Ok(None),
        }
    }

    /**
    Get the time and occupancy% for a range.

    Given a start and end date, return the occupancy data for that range.
    
    It uses the sqlite strftime function to compare the dates with the BETWEEN operator.
    */
    pub fn query_range(
        connection: &PooledConnection<SqliteConnectionManager>,
        table_name: &str,
        from: NaiveDateTime,
        to: NaiveDateTime
    ) -> rusqlite::Result<Vec<(String, u16)>> {
        // let to = to.to_string();
        // let from = from.to_string();
        let mut statement = connection.prepare(&format!(
            "SELECT time,occupancy FROM {} WHERE strftime('%s', time) BETWEEN strftime('%s', ?1) AND strftime('%s', ?2)",
           table_name 
        ))?;

        let rows = statement.query_map(rusqlite::params![from.to_string(), to.to_string()], |row| {
            let time: String = row.get(0)?;
            let occupancy: u16 = row.get(1)?;
            Ok((time, occupancy))
        })?;
        
        let mut data: Vec<(String, u16)> = Vec::new();
        for row in rows {
            data.push(row?);
        }
        Ok(data)
    }

    /**
    Deletes all records specified by the range.

    Uses the sqlite strftime function to compare the dates with the BETWEEN operator.
    */
    pub fn delete_range(
        connection: &PooledConnection<SqliteConnectionManager>,
        table_name: &str,
        from: NaiveDateTime,
        to: NaiveDateTime,
    ) -> rusqlite::Result<()> {
        let from = from.format(ISO_FORMAT).to_string();
        let to = to.format(ISO_FORMAT).to_string();
        connection.execute(
            &format!(
                "DELETE FROM {} WHERE strftime('%s', time) BETWEEN strftime('%s', ?1) AND strftime('%s', ?2)",
                table_name
            ),
            rusqlite::params![from, to],
        )?;
        Ok(())
    }

    
    /**
    Insert one occupancy data into the database.
    */
    pub fn insert_one_occupancy(
        connection: &PooledConnection<SqliteConnectionManager>,
        table_name: &str,
        time: NaiveDateTime,
        occupancy: u16
    ) -> rusqlite::Result<()> {
        connection.execute(
            &format!(
                "INSERT INTO {} (time, occupancy) VALUES (?1, ?2)",
                table_name
            ),
            rusqlite::params![time.format(ISO_FORMAT).to_string(), occupancy],
        )?;
        Ok(())
    }

    
    /**
    Insert many occupancy data into the database.

    `data` is a `Vec` of tuples of (time, occupancy).
    */
    pub fn insert_many_occupancy(
        connection: &PooledConnection<SqliteConnectionManager>,
        table_name: &str,
        data: Vec<(NaiveDateTime, u16)>
    ) -> rusqlite::Result<()> {
        let mut statement = connection.prepare(&format!(
            "INSERT INTO {} (time, occupancy) VALUES (?1, ?2)",
            table_name
        ))?;

        for (time, occupancy) in data {
            statement.execute(rusqlite::params![time.format(ISO_FORMAT).to_string(), occupancy])?;
        }
        Ok(())
    }

}
