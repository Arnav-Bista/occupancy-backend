import sys
import json
import pickle
import pandas as pd
import numpy as np
from datetime import datetime, timedelta

# Classes from notebook
class Timing:
    def __init__(self, opening, closing, isOpen):
        self.opening = opening
        self.closing = closing
        self.isOpen = isOpen

class Schedule:
    def __init__(self, sch):
        self.default = [
            Timing(630, 2230, True),
            Timing(630, 2230, True),
            Timing(630, 2230, True),
            Timing(630, 2230, True),
            Timing(630, 2230, True),
            Timing(800, 2100, True),
            Timing(800, 2100, True),
        ]
        self.data = {}
        for i in range(len(sch)):
            date = sch[0][i]
            timings = []
            jsondata = json.loads(sch[1][i])['timings']
            for j in range(7):
                jsontime = jsondata[j]
                timings.append(Timing(jsontime['opening'], jsontime['closing'], jsontime['open']))
            self.data[date] = timings
            
    def timestamp_to_hhmm(self, timestamp):
        return timestamp.hour * 100 + timestamp.minute
    
    def get_start_of_week(self, timestamp):
        days_since_monday = timestamp.weekday()
        monday = timestamp - pd.Timedelta(days=days_since_monday)
        return monday.normalize()
    
    def get_weekday(self, timestamp):
        return timestamp.weekday()
    
    def get_number(self, timestamp):
        number = self.timestamp_to_hhmm(timestamp)
        start = self.get_start_of_week(timestamp)
        index = self.get_weekday(timestamp)
        
        if start in self.data:
            opening = self.data[start][index].opening
            closing = self.data[start][index].closing
        else:
            opening = self.default[index].opening
            closing = self.default[index].closing
            
        if number <= opening:
            return 0
        elif number >= closing:
            return 1
        else:
            return (number - opening) / (closing - opening)

def extract_features_dates(df):
    max_day = 31
    max_month = 12
    max_day_of_year = 365
    dow = df['timestamp'].dt.dayofweek
    day = df['timestamp'].dt.day
    month = df['timestamp'].dt.month
    year = df['timestamp'].dt.dayofyear
    
    df['day_sin'] = np.sin(dow * (2 * np.pi / 7))
    df['day_cos'] = np.cos(dow * (2 * np.pi / 7))
    
    df['month_sin'] = np.sin(2 * np.pi * month / max_month)
    df['month_cos'] = np.cos(2 * np.pi * month / max_month)
    
    df['day_of_year_sin'] = np.sin(2 * np.pi * year / max_day_of_year)
    df['day_of_year_cos'] = np.cos(2 * np.pi * year / max_day_of_year)
    
    hours = df['timestamp'].dt.hour
    minutes = df['timestamp'].dt.minute
    seconds = df['timestamp'].dt.second
    
    seconds_since_midnight = hours * 3600 + minutes * 60 + seconds
    df['day_progress'] = seconds_since_midnight / 86400

def generate_datetime_range(date_from, date_to, interval_minutes=5):
    current = date_from
    while current <= date_to:
        yield current
        current += timedelta(minutes=interval_minutes)

# Main function
def main():
    # Get input parameters from command line
    if len(sys.argv) != 4:
        print("Usage: python script.py 'YYYY-MM-DD HH:MM' 'YYYY-MM-DD HH:MM' 'schedule_json_string'")
        sys.exit(1)
    
    date_from_str = sys.argv[1]
    date_to_str = sys.argv[2]
    schedule_json_str = sys.argv[3]
    
    # Parse dates
    date_from = datetime.strptime(date_from_str, '%Y-%m-%d %H:%M')
    date_to = datetime.strptime(date_to_str, '%Y-%m-%d %H:%M')
    
    # Parse schedule
    schedule_data = json.loads(schedule_json_str)
    schedule = Schedule(schedule_data)
    
    # Load model
    model = pickle.load(open('gb_model.pkl', 'rb'))
    
    # Generate timestamps for prediction
    timestamps = list(generate_datetime_range(date_from, date_to, 5))
    
    # Create DataFrame
    df = pd.DataFrame({'timestamp': timestamps})
    
    # Extract features
    extract_features_dates(df)
    
    # Add schedule feature
    df['schedule'] = df['timestamp'].apply(lambda x: schedule.get_number(x))
    
    # Prepare input for prediction
    X_pred = df.drop('timestamp', axis=1)
    
    # Make predictions
    predictions = model.predict(X_pred)
    
    # Clip predictions
    predictions = np.clip(predictions, 0, 1)
    
    # Format output
    output_df = pd.DataFrame({
        'datetime': df['timestamp'].dt.strftime('%Y-%m-%d %H:%M:%S'),
        'occupancy': predictions
    })
    
    # Write to output.csv
    output_df.to_csv('output.csv', index=False, header=False)

if __name__ == "__main__":
    main()
