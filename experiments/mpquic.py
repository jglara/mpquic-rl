from tkinter import E
from core.experiment import Experiment
import os
import subprocess

from logging import info

class QuicheQuic(Experiment):
    NAME = "quichequic"    
    
    def __init__(self, net, params):
        super(QuicheQuic, self).__init__(net, params)
        self.server_log = params['log']['path'] + "server.log"
        self.client_log = params['log']['path'] + "client.log"
        self.loglevel = params['log']['level']
        self.file_path = params['file']['path']
        self.file_size = params['file']['size']
        self.repeat = params['repeat']
        self.scheduler = params['scheduler']

    def prepare(self):
        super(QuicheQuic, self).prepare()
        self.net.getNodeByName('s1').cmd("truncate -s {size} {path}".format(
            size= self.file_size, 
            path= self.file_path))

        self.net.getNodeByName("h1").cmd("rm {}".format(self.client_log))
        self.net.getNodeByName("s1").cmd("rm {}".format(self.client_log))
    
        
    def get_server_cmd(self):
        cmd="RUST_LOG={loglevel} {quichepath}/target/debug/mp_server --listen 10.0.3.10:4433 --cert {quichepath}/apps/src/bin/cert.crt --key {quichepath}/apps/src/bin/cert.key --root {wwwpath} --scheduler {sched}>> {log}&".format(
            quichepath='../quiche', 
            sched=self.scheduler,
            wwwpath='./',
            log=self.server_log,
            loglevel=self.loglevel)

        print(cmd)
        return cmd

    def get_client_cmd(self):
        cmd="RUST_LOG={loglevel} {quichepath}/target/debug/mp_client -l 10.0.1.1:5555 -w 10.0.2.1:6666 --url https://10.0.3.10:4433/{file} >> {log}".format(
            quichepath='../quiche', 
            file=self.file_path,
            log=self.client_log,
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
            self.net.getNodeByName('s1').cmd(self.get_server_cmd())
            self.net.getNodeByName('h1').cmd(self.get_client_cmd())