use std::{
    fs,
    process::{Command, Stdio},
};

use crate::{
    timing::schedule::{self, Schedule},
    ISO_FORMAT,
};
use chrono::{NaiveDate, NaiveDateTime};

pub struct GBRegressor {}

impl GBRegressor {
    pub fn predict_gym(
        from: NaiveDate,
        to: NaiveDate,
        schedule: &Schedule,
    ) -> Result<Vec<(NaiveDateTime, f64)>, String> {
        let command = match Command::new("bash")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .args([
                "./make_gb_predictions.bash",
                &from.to_string(),
                &to.to_string(),
                &serde_json::to_string(schedule).unwrap(),
            ])
            .spawn()
        {
            Ok(command) => command,
            Err(e) => return Err(format!("Failed to spawn. {}", e.to_string())),
        };

        match command.wait_with_output() {
            Ok(output) => {
                if !output.status.success() {
                    return Err(format!("Failed to execute the command. {}", output.status));
                }
            }
            Err(e) => return Err(format!("Failed to wait. {}", e.to_string())),
        }
        let output = match fs::read_to_string("gb_prediction/output") {
            Ok(output) => output,
            Err(e) => return Err(format!("Failed to read output. {}", e.to_string())),
        };

        let mut predictions: Vec<(NaiveDateTime, f64)> = Vec::new();
        for line in output.lines() {
            let mut split = line.split(',');
            let date = split.next().unwrap();
            let occupancy = split.next().unwrap();

            let date: NaiveDateTime = NaiveDateTime::parse_from_str(date, ISO_FORMAT).unwrap();
            let occupancy: f64 = occupancy.parse().unwrap();

            predictions.push((date, occupancy));
        }

        Ok(predictions)
    }
}
