#!/bin/bash


cd ./lstm_prediction
source ./venv/bin/activate
python3 make_predictions.py $1 $2 $3

