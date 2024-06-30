use serde::Serialize;

use crate::timing::schedule::Schedule;

/// The Response struct that is used to send data back to the client.
///
/// This is pretty basic so far.

#[derive(Serialize, Clone)]
pub struct MyResponse {
    data: Vec<(String, u16)>,
    prediction_knn: Vec<(String, u16)>,
    schedule: Schedule,
}

impl MyResponse {
    pub fn new(data: Vec<(String, u16)>, schedule: Schedule, prediction_knn: Vec<(String, u16)>) -> Self {
        Self {
            data,
            schedule,
            prediction_knn
        }
    }
}
