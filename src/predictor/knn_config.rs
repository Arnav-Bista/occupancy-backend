use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct KNNConfig {
    last_scraped: String,
}

impl KNNConfig {
    pub fn new(last_scraped: String) -> Self {
        Self { last_scraped }
    }

    pub fn new_naive_date(last_scraped: NaiveDate) -> Self {
        let timestamp = last_scraped.format("%Y-%m-%dT%H:%M:%S").to_string();
        Self {
            last_scraped: timestamp,
        }
    }

    pub fn get_last_scraped(&self) -> String {
        self.last_scraped.clone()
    }

    pub fn get_last_scraped_naive_date(&self) -> NaiveDate {
        NaiveDate::parse_from_str(&self.last_scraped, "%Y-%m-%dT%H:%M:%S").unwrap()
    }
}
