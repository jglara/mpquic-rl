#!/bin/bash

rm -rf logs*
python3 ./runner.py -t topo.yaml -x experiment_rr.yaml
python3 ./runner.py -t topo.yaml -x experiment_rtt.yaml

