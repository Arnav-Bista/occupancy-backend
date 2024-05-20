use chrono::{DateTime, Local};
use chrono_tz::Tz;


pub fn uk_datetime_now() -> DateTime<Tz> {
    let local_datetime = Local::now();
    let uk_timezone: Tz = "Europe/London".parse().unwrap();
    let uk_datetime: DateTime<Tz> = local_datetime.with_timezone(&uk_timezone);
    return uk_datetime;
}
