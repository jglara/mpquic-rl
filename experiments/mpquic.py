from tkinter import E
from core.experiment import Experiment
import os
import subprocess

from logging import info


class QuicheQuic(Experiment):
    NAME = "quichequic"    
    QUICHEPATH = "./mpquic-quiche"
    
    def __init__(self, net, params, output_dir):
        super(QuicheQuic, self).__init__(net, params, output_dir)
        
        self.loglevel = params['log']['level']
        self.file_path = params['file']['path']
        self.file_size = params['file']['size']
        self.repeat = params['repeat']
        self.scheduler = params['scheduler']
        self.qlog = params['qlog']

    def prepare(self):
        super(QuicheQuic, self).prepare()
        self.net.getNodeByName('s1').cmd("truncate -s {size} {path}".format(
            size= self.file_size, 
            path= self.file_path))

        
    def get_server_cmd(self, instance):
        #QLOGDIR={csv_path} 
        cmd="{qlog} RUST_LOG={loglevel} {quichepath}/target/debug/mp_server --listen 10.0.3.10:4433 --cert {quichepath}/src/bin/cert.crt --key {quichepath}/src/bin/cert.key --root {wwwpath} --scheduler {sched} --path-stats-output {output}/path-{i}.csv --conn-stats-output {output}/conn-{i}.csv --sched-stats-output {output}/{sched}-{i}.csv > {output}/server{i}.log&".format(
            qlog="QLOGDIR={}".format(self.output_dir) if self.qlog else "",
            output=self.output_dir,
            i=instance,
            quichepath=self.QUICHEPATH, 
            sched=self.scheduler,
            wwwpath='./',
            loglevel=self.loglevel)

        print(cmd)
        return cmd

    def get_client_cmd(self,instance):
        #SSLKEYLOGFILE={csv_path}/keylog{i}.log 
        #QLOGDIR={csv_path} 
        cmd="{qlog} RUST_LOG={loglevel} {quichepath}/target/debug/mp_client -l 10.0.1.1:5555 -w 10.0.2.1:6666 --url https://10.0.3.10:4433/{file} --download-stats-output {output}/download-{i}.csv --path-stats-output {output}/client-{i}.csv > {output}/client{i}.log".format(
            qlog="QLOGDIR={}".format(self.output_dir) if self.qlog else "",
            output=self.output_dir,
            i=instance,
            quichepath=self.QUICHEPATH, 
            file=self.file_path,
            loglevel=self.loglevel)
        
        print(cmd)
        return cmd

    def clean(self):
        super(QuicheQuic, self).clean()
        self.net.getNodeByName('s1').cmd("rm {path}".format(
            path= self.file_path))
        

    def run(self):
        import time
        n = self.file_size = self.repeat
        for i in range(0,n):
            time.sleep(1)
            self.net.getNodeByName('s1').cmd(self.get_server_cmd(i))
            time.sleep(1)
            self.net.getNodeByName('h1').cmd(self.get_client_cmd(i))