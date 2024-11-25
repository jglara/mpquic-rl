 #!/usr/bin/env python3
import logging
import argparse
import traceback
from core.basicTopo import MultipathTopo
from mininet.node import OVSBridge
from mininet.clean import cleanup
from mininet.net import Mininet
from mininet.cli import CLI
import yaml
from experiments import EXPERIMENTS
from core.experiment import Experiment
import os

class Runner(object):
    """
    Run an experiment described by `experiment_parameter_file` in the topology
    described by `topo_parameter_file` 
    """
    def __init__(self, topo_parameter_file, experiment_parameter_file, output_dir):
        self.output_dir = output_dir
        self.topo_params = yaml.safe_load(open(topo_parameter_file))['topo']

        self.topo = MultipathTopo(**self.topo_params)
        self.net = Mininet(self.topo, switch=OVSBridge, controller=None)
    
        self.start_topo()
        if experiment_parameter_file == None:
            CLI(self.net)
        else:
            self.exp_params = yaml.safe_load(open(experiment_parameter_file))['experiment']
            self.run_experiment()

        self.stop_topo()

    def start_topo(self):
        """
        Initialize the topology 
        """
        self.net.start()
    
    def run_experiment(self):
        """
        Match the name of the experiment and launch it
        """
        #import pdb; pdb.set_trace()

        try:
            os.mkdir(self.output_dir)
        except OSError as error:
            pass

        xp = self.exp_params["type"]
        if xp in EXPERIMENTS:
            exp = EXPERIMENTS[xp](self.net, self.exp_params, self.output_dir)
            exp.classic_run()
        else:
            raise Exception("Unknown experiment {}".format(xp))

    def stop_topo(self):
        """
        Stop the topology
        """
        self.net.stop()

if __name__ == '__main__':

    parser = argparse.ArgumentParser(
        description="Multipath topo")

    parser.add_argument("--topo_param_file", "-t", required=True,
        help="path to the topo parameter file")
    parser.add_argument("--experiment_param_file", "-x", nargs="?",
        help="path to the experiment parameter file")
    parser.add_argument("--output_dir", "-o", required=True,
        help="path to dump output files")


    args = parser.parse_args()

    #logging.basicConfig(format="%(asctime)-15s [%(levelname)s] %(funcName)s: %(message)s", level=logging.INFO)

    try:
        Runner(args.topo_param_file, args.experiment_param_file, args.output_dir)
    except Exception as e:
        logging.fatal("A fatal error occurred: {}".format(e))
        traceback.print_exc()
    finally:
        # Always cleanup Mininet
        logging.info("cleanup mininet")
        cleanup()