use std::sync::Arc;

use chrono::NaiveDate;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

use crate::ISO_FORMAT;

pub struct KNNRegressor {
    database_name: String,
}

impl KNNRegressor {
    // pub fn setup(database_name: &str) -> Self {
    //     Self {
    //         database_name: database_name.to_string(),
    //     }
    // }

    pub fn predict_database(
        connection_pool: Arc<Pool<SqliteConnectionManager>>,
        name: String,
        from: NaiveDate,
        to: NaiveDate,
        resolution: f64,
        k: usize,
    ) {
        let connection = match connection_pool.get() {
            Ok(conn) => conn,
            Err(_) => {
                println!("Could not get database connection");
                return;
            }
        };

        // Delete all records in between if existing
        match connection.execute(
            &format!(
                "DELETE FROM {}_prediction_knn WHERE time >= '{}' AND time <= '{}'",
                name,
                from.format(ISO_FORMAT),
                to.format(ISO_FORMAT)
            ),
            (),
        ) {
            Err(_) => {
                println!("Could not delete records");
                return;
            }
            _ => (),
        };
    }

    /**
    Predicts one value using the KNN Regressors algorithm

    Where x is a vector containing (weight, time) and y is the occupancy %.
    The target is the time for which we want to predict.
    k is the number of neighbors to consider.

    The target and the time can be in any numerical format (even HHMM), however, to predict a range, it
    is better to use epoch time to avoid impossible timings such as 0561.

    It uses a modified version of the KNN algorithm. We'll get the nearlest K neighbours using the
    distance between the time and the target alone. Then we'll consider the weights into the equation.
    */
    pub fn predict_one(x: Vec<(f64, f64)>, y: Vec<f64>, target: f64, k: usize) -> f64 {
        // Weight, Time, Occupancy
        let mut neighbours: Vec<(f64, f64, f64)> = Vec::with_capacity(k);

        for (i, (weight, time)) in x.iter().enumerate() {
            if neighbours.len() < k {
                neighbours.push((*weight, *time, y[i]));
                continue;
            }

            let mut max_distance = f64::MIN;
            let mut max_index = 0;

            for i in 0..k {
                let distance = (time - target).abs();
                if distance > max_distance {
                    max_distance = distance;
                    max_index = i;
                }
            }
            neighbours[max_index] = (*weight, *time, y[i]);
        }

        let mut sum = 0.0;
        for (weight, _, occupancy) in neighbours {
            sum += weight * occupancy;
        }
        sum / (k as f64)
    }

    /**
    Predicts a range of values using [predict_one].

    Where `x` is a `Vec` containing (weight, time) and `y` is the occupancy %.
    The `start` and `end` are the range of time for which we want to predict.
    The `resolution` is the step size between each prediction.
    `k` is the number of neighbors to consider.

    As mentioned previously, the target and the time can be in any numerical format, however, to predict
    a range, it is better to use epoch time to avoid impossible timings such as 0561.
    */
    pub fn predict_range(
        x: Vec<(f64, f64)>,
        y: Vec<f64>,
        mut start: f64,
        end: f64,
        resolution: f64,
        k: usize,
    ) -> Vec<(f64, f64)> {
        assert!(start < end, "Start time must be less than end time");

        let mut predictions = Vec::with_capacity(((end - start) / resolution) as usize);
        while start < end {
            predictions.push((start, Self::predict_one(x.clone(), y.clone(), start, k)));
            start += resolution;
        }
        predictions
    }
}
