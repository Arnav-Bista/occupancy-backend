use std::{f64, sync::Arc};

use chrono::NaiveDate;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

pub struct KNNRegressor {}

impl KNNRegressor {
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
        // Weight, Distance, Occupancy
        let mut neighbours: Vec<(f64, f64, f64)> = vec![(0.0, f64::INFINITY, 0.0); k];

        for (i, (weight, time)) in x.iter().enumerate() {
            let distance = (time - target).abs();

            let (max_distance_index, max_distance) = neighbours
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.1.partial_cmp(&b.1).unwrap())
                .unwrap();

            if distance < max_distance.1 {
                neighbours[max_distance_index] = (*weight, distance, y[i]);
            }
        }

        let mut weighted_sum = 0.0;
        let mut weight_sum = 0.0;
        for (weight, _, occupancy) in neighbours {
            weighted_sum += weight * occupancy;
            weight_sum += weight;
        }

        if weight_sum == 0.0 {
            // Avoid division by zero
            // JIC
            return 0.0;
        }

        // Weighted average
        weighted_sum / weight_sum
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
        start: f64,
        end: f64,
        resolution: f64,
        k: usize,
    ) -> Vec<(f64, f64)> {
        assert!(start < end, "Start time must be less than end time");

        let mut start = start;

        let mut predictions = Vec::with_capacity(((end - start) / resolution) as usize);
        while start <= end {
            predictions.push((start, Self::predict_one(x.clone(), y.clone(), start, k)));
            start += resolution;
        }
        predictions
    }
}
