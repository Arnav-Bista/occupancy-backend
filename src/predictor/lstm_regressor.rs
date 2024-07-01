use std::{
    fs,
    process::{Command, Stdio},
};

use chrono::{NaiveDate, NaiveDateTime};

use crate::ISO_FORMAT;

pub struct LSTMRegressor {}

impl LSTMRegressor {
    pub fn predict_gym(
        date: NaiveDate,
        opening: u16,
        closing: u16,
    ) -> Result<Vec<(NaiveDateTime, f64)>, String> {
        let command = match Command::new("bash")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .args([
                "./make_lstm_predictions.bash",
                &date.to_string(),
                &opening.to_string(),
                &closing.to_string(),
            ])
            .spawn()
        {
            Ok(command) => command,
            Err(e) => return Err(format!("Failed to spawn. {}", e.to_string())),
        };

        match command.wait_with_output() {
            Ok(output) => {
                if !output.status.success() {
                    return Err("Failed to execute the command".to_string());
                }
            }
            Err(e) => return Err(format!("Failed to wait. {}", e.to_string())),
        }

        let output = match fs::read_to_string("lstm_prediction/output") {
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
