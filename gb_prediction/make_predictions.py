import sys
import json
import pickle
import pandas as pd
import numpy as np
from datetime import datetime, timedelta

# Used Claude to convert my notebook into this script

def classify_term_time(df, timestamp_col='timestamp'):
    result_df = df.copy()

    # Define term time periods for both academic years
    term_periods = [
        # ====== 2024-2025 Academic Year ======
        # Semester 1 teaching (excluding ILW)
        (pd.Timestamp('2024-09-16'), pd.Timestamp('2024-10-20')),  # Week 1-5
        (pd.Timestamp('2024-10-28'), pd.Timestamp('2024-12-01')),  # Week 7-11
        
        # Semester 1 exams
        (pd.Timestamp('2024-12-06'), pd.Timestamp('2024-12-20')),
        
        # Semester 2 teaching (excluding Spring vacation and ILW)
        (pd.Timestamp('2025-01-27'), pd.Timestamp('2025-03-02')),  # Week 1-5
        (pd.Timestamp('2025-03-10'), pd.Timestamp('2025-04-06')),  # Week 6-9
        (pd.Timestamp('2025-04-14'), pd.Timestamp('2025-04-27')),  # Week 11-12
        
        # Semester 2 revision and exams
        (pd.Timestamp('2025-04-28'), pd.Timestamp('2025-05-26')),  # Revision and regular exams
        (pd.Timestamp('2025-05-27'), pd.Timestamp('2025-05-31'))   # Extended exams (partial week)
    ]
    
    # Initialize the column with all 1's (assuming non-term time by default)
    result_df['is_non_term_time'] = 1
    
    # Set to 0 for dates that fall within term periods
    for start_date, end_date in term_periods:
        # Add one day to end_date to make the comparison inclusive
        end_date_inclusive = end_date + pd.Timedelta(days=1)
        mask = (result_df[timestamp_col] >= start_date) & (result_df[timestamp_col] < end_date_inclusive)
        result_df.loc[mask, 'is_non_term_time'] = 0
    
    return result_df

# Classes from notebook
class Timing:
    def __init__(self, opening, closing, isOpen):
        self.opening = opening
        self.closing = closing
        self.isOpen = isOpen

class Schedule:
    def __init__(self, sch_json):
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
        jsondata = json.loads(sch_json)['timings']
        timings = []
        for jsontime in jsondata:
            timings.append(Timing(jsontime['opening'], jsontime['closing'], jsontime['open']))
        self.data = timings
            
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

        index = self.get_weekday(timestamp)
        
        opening = self.data[index].opening
        closing = self.data[index].closing
            
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
        print("Usage: python script.py 'YYYY-MM-DD' 'YYYY-MM-DD' 'schedule_json_string'")
        sys.exit(1)
    
    date_from_str = sys.argv[1]
    date_to_str = sys.argv[2]
    schedule_json_str = sys.argv[3]
    
    # Parse dates
    date_from = datetime.strptime(date_from_str, '%Y-%m-%d')
    date_to = datetime.strptime(date_to_str, '%Y-%m-%d')
    
    # Parse schedule
    schedule = Schedule(schedule_json_str)
    
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
    
    # Add term time classification
    df = classify_term_time(df)
    
    # Prepare input for prediction
    X_pred = df.drop('timestamp', axis=1)
    
    # Make predictions
    predictions = model.predict(X_pred)
    
    # Clip predictions
    predictions = np.clip(predictions, 0, 1)
    
    # Format output
    output_df = pd.DataFrame({
        'datetime': df['timestamp'].dt.strftime('%Y-%m-%d %H:%M:%S'),
        'occupancy': predictions * 100  
    })
    
    # Write to output.csv
    output_df.to_csv('output.csv', index=False, header=False)

if __name__ == "__main__":
    main()
