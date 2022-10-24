import logging
import os


class Experiment(object):
    """
	Base class to instantiate an experiment to perform.
	This class is not instantiable as it. You must define a child class with the
	`NAME` attribute.
	"""
    
    def __init__(self, net, parameters, output_dir):
        """
        Instantiation of this base class only load the experiment parameter
        """
        self.parameters = parameters
        self.net = net
        self.output_dir = output_dir

    def classic_run(self):
        """
        Default function to perform the experiment. It consists into three phases:
        - A preparation phase through `prepare()` (generating experiment files,...)
        - A running phase through `run()` (where the actual experiment takes place)
        - A cleaning phase through `clean()` (stopping traffic, removing generated files,...)
        """
        self.prepare()
        self.run()
        self.clean()

    def prepare(self):
        """
        Prepare the environment to run the experiment.
        Typically, when you inherit from this class, you want to extend this
        method, while still calling this parent function.        
        """
        self.run_tcpdump()


    def run(self):
        """
        Perform the experiment
        This function MUST be overriden by child classes
        """
        raise NotImplementedError("Trying to run Experiment")

    def clean(self):
        """
        Clean the environment where the experiment took place.
        Typically, when you inherit from this class, you want to extend this
        method, while still calling this parent function.
        """
        self.net.getNodeByName("h1").cmd("killall tcpdump")
        self.net.getNodeByName("s1").cmd("killall tcpdump")
    
    def run_tcpdump(self):
        client_pcap = self.parameters['pcap']['client']
        server_pcap = self.parameters['pcap']['server']
        snaplen_pcap = self.parameters['pcap']['snaplen']


        #import pdb; pdb.set_trace()
        
        if client_pcap:
            cmd = "tcpdump -i any -s {} -w {}/client.pcap &".format(snaplen_pcap, self.output_dir)
            #print(cmd)
            self.net.getNodeByName("h1").cmd(cmd)
        if server_pcap:
            cmd="tcpdump -i any -s {} -w {}/server.pcap &".format(snaplen_pcap, self.output_dir)
            #print(cmd)
            self.net.getNodeByName("h1").cmd(cmd)
        if server_pcap or client_pcap:
            logging.info("Activating tcpdump")
            