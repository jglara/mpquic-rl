from tkinter import E
from core.experiment import Experiment
import os
import subprocess

from logging import info


class QuicheQuic(Experiment):
    NAME = "quichequic"    
    QUICHEPATH = "./mpquic-quiche"
    
    def __init__(self, net, params):
        super(QuicheQuic, self).__init__(net, params)
        
        self.log_path = params['log']['path']
        self.loglevel = params['log']['level']
        self.file_path = params['file']['path']
        self.file_size = params['file']['size']
        self.repeat = params['repeat']
        self.scheduler = params['scheduler']

        try:
            os.mkdir(self.log_path)
        except OSError as error:
            pass

    def prepare(self):
        super(QuicheQuic, self).prepare()
        self.net.getNodeByName('s1').cmd("truncate -s {size} {path}".format(
            size= self.file_size, 
            path= self.file_path))

        
    def get_server_cmd(self, instance):
        cmd="RUST_LOG={loglevel} {quichepath}/target/debug/mp_server --listen 10.0.3.10:4433 --cert {quichepath}/src/bin/cert.crt --key {quichepath}/src/bin/cert.key --root {wwwpath} --scheduler {sched} --path-stats-output {csv_path}/{sched}{i}.csv > {log}/server{i}.log&".format(
            csv_path=self.log_path,
            i=instance,
            quichepath=self.QUICHEPATH, 
            sched=self.scheduler,
            wwwpath='./',
            log=self.log_path,
            loglevel=self.loglevel)

        print(cmd)
        return cmd

    def get_client_cmd(self,instance):
        cmd="SSLKEYLOGFILE={csv_path}/keylog{i}.log RUST_LOG={loglevel} {quichepath}/target/debug/mp_client -l 10.0.1.1:5555 -w 10.0.2.1:6666 --url https://10.0.3.10:4433/{file} --download-stats-output {csv_path}/download{i}.csv> {log}/client{i}.log".format(
            csv_path=self.log_path,
            i=instance,
            quichepath=self.QUICHEPATH, 
            file=self.file_path,
            log=self.log_path,
            loglevel=self.loglevel)
        
        print(cmd)
        return cmd

    def clean(self):
        super(QuicheQuic, self).clean()
        self.net.getNodeByName('s1').cmd("rm {path}".format(
            path= self.file_path))
        

    def run(self):
        n = self.file_size = self.repeat
        for i in range(0,n):
            self.net.getNodeByName('s1').cmd(self.get_server_cmd(i))
            self.net.getNodeByName('h1').cmd(self.get_client_cmd(i))