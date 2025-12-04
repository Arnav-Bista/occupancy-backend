use std::fmt::Display;

use chrono::{NaiveDate, NaiveDateTime};
use headless_chrome::Browser;
use regex::Regex;
use reqwest::Client;
use reqwest::{Method, RequestBuilder};

use crate::{
    scraper::scraper::Scrape,
    timing::{daily::Daily, schedule::Schedule},
};
use crate::{ISO_FORMAT, ISO_FORMAT_DATE};

pub struct Gym {
    url: String,
    user_agent: String,
    client: Client,
    last_scraped: Option<NaiveDate>,
    // Man I love regex
    occupancy_regex: Regex,
    schedule_regex: Regex,
    schedule_entry_regex: Regex,
    schedule_time_regex: Regex,
}

impl Gym {
    pub fn new(last_scraped: Option<String>) -> Self {
        let last_scraped = match last_scraped {
            Some(date) => Some(NaiveDate::parse_from_str(&date, ISO_FORMAT_DATE).unwrap()),
            None => None,
        };

        Self {
            url: "https://sport.wp.st-andrews.ac.uk/".to_string(),
            user_agent: "Mozilla/5.0".to_string(),
            client: Client::new(),
            last_scraped,
            // 🗿
            occupancy_regex: Regex::new(r"Occupancy:\s+(\d+)%").unwrap(),
            schedule_regex: Regex::new("<dd class=\"paired-values-list__value\">(.*?)</dd>")
                .unwrap(),
            schedule_entry_regex: Regex::new(r"(.*)\sto\s(.*)|CLOSED").unwrap(),
            schedule_time_regex: Regex::new(r"(\d+).(\d+)(.*)").unwrap(),
        }
    }

    fn parse_timings(&self, string: &str) -> u16 {
        // HHMM - 24 Hour Format
        let regex_match = self.schedule_time_regex.captures(string).unwrap();
        // The regex checks for these errors 🗿
        let hour: u16 = regex_match.get(1).unwrap().as_str().parse().unwrap();
        let minute: u16 = regex_match.get(2).unwrap().as_str().parse().unwrap();
        let ampm = regex_match.get(3).unwrap().as_str().to_lowercase();

        match ampm.as_str() {
            "am" => hour * 100 + minute,
            "pm" => (hour + 12) * 100 + minute,
            _ => 0,
        }
    }
}

impl Scrape<Gym> for Gym {
    fn table_name() -> String {
        "gym".to_string()
    }

    fn parse_occupancy(&self, body: &str) -> Option<u16> {
        let regex_match = match self.occupancy_regex.captures(&body) {
            Some(data) => data,
            None => {
                println!("Occupancy Scrape Error. Regex Fail");
                return None;
            }
        };
        let result: &str = regex_match.get(1).map_or("0", |m| m.as_str());
        let result: u16 = match result.parse() {
            Ok(num) => num,
            Err(_) => {
                println!("Occupancy Scrape Error. Parse to u16 fail.");
                return None;
            }
        };
        Some(result)
    }

    fn parse_schedule(&self, body: &str) -> Option<Schedule> {
        // Captures the tags encompassing the Schedule
        let mut schedules = self.schedule_regex.captures_iter(body);
        let mut schedule = Schedule::new();
        while let Some(inner_html) = schedules.next() {
            // Capture each row
            let timings = self
                .schedule_entry_regex
                .captures(inner_html.get(1)?.as_str())?;
            // Captures the Numbers
            // Opening
            let timings_match: &str = timings.get(1)?.as_str();
            if timings_match == "CLOSED" {
                let _ = schedule.add_timing(Daily::new_closed());
                continue;
            }
            let opening = timings_match;
            // Closing
            let closing = timings.get(2)?.as_str();
            let _ = schedule.add_timing(Daily::new_open(
                self.parse_timings(opening),
                self.parse_timings(closing),
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

    fn fetch_data(&self) -> Result<String, String> {
        let browser = Browser::default().map_err(|_| "Could not create a browser")?;

        let tab = browser.new_tab().map_err(|_| "Could not create a tab")?;
        tab.navigate_to(&self.url)
            .map_err(|_| "Coult not navigate")?;
        tab.wait_until_navigated().map_err(|_| "Could not wait")?;
        match tab.get_content().map_err(|_| "Could not get content") {
            Ok(html) => Ok(html),
            Err(err) => Err(err.into()),
        }
    }
}
