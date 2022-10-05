
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

class Runner(object):
    """
    Run an experiment described by `experiment_parameter_file` in the topology
    described by `topo_parameter_file` 
    """
    def __init__(self, topo_parameter_file, experiment_parameter_file):
        self.topo_params = yaml.safe_load(open(topo_parameter_file))['topo']
        

        self.topo = MultipathTopo(**self.topo_params)
        self.net = Mininet(self.topo, switch=OVSBridge, controller=None)
        #net.addNAT().configDefault()
    
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
        xp = self.exp_params["type"]
        if xp in EXPERIMENTS:
            exp = EXPERIMENTS[xp](self.net, self.exp_params)
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
    parser.add_argument("--experiment_param_file", "-x",
        help="path to the experiment parameter file")

    args = parser.parse_args()

    #logging.basicConfig(format="%(asctime)-15s [%(levelname)s] %(funcName)s: %(message)s", level=logging.INFO)

    try:
        Runner(args.topo_param_file, args.experiment_param_file)
    except Exception as e:
        logging.fatal("A fatal error occurred: {}".format(e))
        traceback.print_exc()
    finally:
        # Always cleanup Mininet
        logging.info("cleanup mininet")
        cleanup()