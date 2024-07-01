import tensorflow as tf
import numpy as np
from datetime import datetime, timedelta
import sys

import matplotlib.pyplot as plt

TIME_INTERVAL = timedelta(minutes=5)
SEQUENCE_LENGTH = 12 * 4


def create_sequences(features, targets, sequence_length):
    X, y = [], []
    for i in range(len(features) - sequence_length):
        X.append(features[i:i + sequence_length])
        y.append(targets[i + sequence_length])
    return np.array(X), np.array(y)


def main():
    # Error Handling
    if len(sys.argv) != 4:
        print('Usage: python make_predictions.py <date> <opening> <closing>')
        print("Where: date is in the format 'YYYY-MM-DD', and Opening and Closing are in the format 'HHMM'")
        sys.exit(1)

    day = datetime.strptime(sys.argv[1], '%Y-%m-%d')
    opening = int(sys.argv[2])
    closing = int(sys.argv[3])

    opening_datetime = datetime(day.year, day.month, day.day, opening // 100, opening % 100)
    closing_datetime = datetime(day.year, day.month, day.day, closing // 100, closing % 100)

    print(opening_datetime)
    print(closing_datetime)

    timings = []
    seasonal = (day - datetime(day.year, 1, 1)).days / 365
    current_time = opening_datetime
    while current_time < closing_datetime:
        normalised = (current_time.timestamp() - opening_datetime.timestamp()) / (closing_datetime.timestamp() - opening_datetime.timestamp())
        timings.append([normalised, seasonal])
        current_time += TIME_INTERVAL

    timings = np.array(timings)

    X = create_sequences(timings, [0] * len(timings), SEQUENCE_LENGTH)[0]

    model = tf.keras.models.load_model('model_3lstm.keras')
    pred = model.predict(X)
    
    with open("output", "w") as f:
        for val in pred:
            f.write(str(val[0]) + '\n')

    print(pred.transpose())


if __name__ == '__main__':
    main()
