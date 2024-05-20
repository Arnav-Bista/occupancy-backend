use chrono::{DateTime, Datelike, Timelike, Weekday};
use chrono_tz::Tz;

use tokio::time::{Instant, Duration};
use super::daily::Daily;


#[derive(Debug)]
pub struct Schedule {
    timings: [Daily; 7],
    count: usize,
    standard_interval_min: u16
}


impl Schedule {
    pub fn new() -> Self {
        Self {
            timings: [Daily::new_closed(); 7],
            count: 0,
            standard_interval_min: 5
        }
    }

    pub fn add_timing(&mut self, timing: Daily) ->  Result<(),()> {
        if self.count >= 7 { return Err(()) }
        self.timings[self.count] = timing;
        self.count += 1;
        Ok(())
    }

    pub fn is_open(&self, timestamp: DateTime<Tz>) -> bool {
        let weekday = timestamp.weekday().number_from_monday() - 1;
        let daily = self.timings[weekday as usize];
        if !daily.open() {
            return false;
        };
        let hm = timestamp.hour() * 100 + timestamp.minute();
        let hm: u16 = hm as u16;
        if daily.opening().unwrap() <= hm && hm <= daily.closing().unwrap() {
            return true;
        }
        return false;
    }

    fn convert_hm_to_sec(hm: u16) -> u64 {
        let min_part = hm % 100;
        let mut total: u64 = min_part as u64 * 60;
        total += (hm - min_part) as u64 * 60 * 60;
        total
    }
}
