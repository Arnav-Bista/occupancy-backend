use chrono::{DateTime, NaiveDate, NaiveDateTime};
use chrono_tz::Tz;
use regex::Regex;
use reqwest::{Client, Method, RequestBuilder};
use serde::Deserialize;

use crate::{
    scraper::scraper::Scrape,
    timing::{daily::Daily, schedule::Schedule, uk_datetime_now::uk_datetime_now},
    ISO_FORMAT_DATE,
};

pub struct MainLibrary {
    url: String,
    schedule_url: String,
    client: Client,
    user_agent: String,
    last_scraped: Option<NaiveDate>,
    // Some more regex
    schedule_regex: Regex,
    schedule_entry_regex: Regex,
}

#[derive(Deserialize, Debug)]
struct APIResponse {
    pub staff: u32,
    pub other: u32,
    pub student: u32,
    pub total: u32,
    pub capacity: u32,
}

impl MainLibrary {
    pub fn new(last_scraped: Option<String>) -> Self {
        let last_scraped = match last_scraped {
            Some(date) => Some(NaiveDate::parse_from_str(&date, ISO_FORMAT_DATE).unwrap()),
            None => None,
        };

        Self {
            url: "https://www.st-andrews.ac.uk/library/sentry-api/current-occupancy".to_string(),
            schedule_url: "https://www.st-andrews.ac.uk/library/".to_string(),
            user_agent: "Mozilla/5.0".to_string(),
            client: Client::new(),
            last_scraped,
            schedule_regex: Regex::new("<dd class=\"paired-values-list__value\">(.*?)</dd>")
                .unwrap(),
            schedule_entry_regex: Regex::new(r"(\d+)(\w+)\sto\s(\d+)(\w+)|CLOSED").unwrap(),
        }
    }

    fn parse_timings(&self, string: &str, ampm: &str) -> u16 {
        // HHMM - 24 Hour Format
        let hour: u16 = string.parse().unwrap();

        match ampm {
            "am" => hour * 100,
            "pm" => (hour + 12) * 100,
            _ => 0,
        }
    }
}

impl Scrape<MainLibrary> for MainLibrary {
    fn table_name() -> String {
        "main_library".to_string()
    }

    fn get_request(&self) -> RequestBuilder {
        self.client
            .request(Method::GET, &self.url)
            .header("User-Agent", &self.user_agent)
    }

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

        let schedule_response = match self
            .client
            .request(Method::GET, &self.schedule_url)
            .send()
            .await
        {
            Ok(data) => data,
            Err(err) => return Err(err.to_string()),
        };

        let schedule_body = match schedule_response.text().await {
            Ok(text) => text,
            Err(err) => return Err(err.to_string()),
        };

        Ok((
            Self::parse_occupancy(&self, &body),
            Self::parse_schedule(&self, &schedule_body),
            timestamp,
        ))
    }

    fn parse_occupancy(&self, body: &str) -> Option<u16> {
        let response: APIResponse = match serde_json::from_str(body) {
            Err(_) => return None,
            Ok(data) => data,
        };
        Some(((response.total * 100) / response.capacity) as u16)
    }

    fn parse_schedule(&self, body: &str) -> Option<Schedule> {
        let mut schedules = self.schedule_regex.captures_iter(body);
        let mut schedule = Schedule::new();
        let mut counter = 0;
        while let Some(inner_html) = schedules.next() {
            if counter >= 7 {
                break;
            }
            counter += 1;
            let timings = self
                .schedule_entry_regex
                .captures(inner_html.get(1)?.as_str())?;
            let timings_match: &str = timings.get(1)?.as_str();
            if timings_match == "CLOSED" {
                let _ = schedule.add_timing(Daily::new_closed());
                continue;
            }
            let _ = schedule.add_timing(Daily::new_open(
                self.parse_timings(
                    timings.get(1).unwrap().as_str(),
                    timings.get(2).unwrap().as_str(),
                ),
                self.parse_timings(
                    timings.get(3).unwrap().as_str(),
                    timings.get(4).unwrap().as_str(),
                ),
            ));
        }
        Some(schedule)
    }

    fn set_last_updated(&mut self, last_updated: NaiveDate) {
        self.last_scraped = Some(last_updated);
    }

    fn get_last_updated(&self) -> Option<NaiveDate> {
        self.last_scraped
    }
}
