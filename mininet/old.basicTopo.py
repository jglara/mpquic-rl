from logging import info
from subprocess import PIPE, Popen
from mininet.topo import Topo
from mininet.link import Link
from mininet.link import TCIntf
from mininet.node import Node, Host, OVSBridge
from mininet.net import Mininet
from mininet.cli import CLI

class MyTCLink( Link ):
    "Link with symmetric TC interfaces configured via opts"
    def __init__( self, node1, node2, port1=None, port2=None,
                  intfName1=None, intfName2=None,
                  addr1=None, addr2=None, ip1=None, ip2=None, **params ):
        Link.__init__( self, node1, node2, port1=port1, port2=port2,
                       intfName1=intfName1, intfName2=intfName2,
                       cls1=TCIntf,
                       cls2=TCIntf,
                       addr1=addr1, addr2=addr2,
                       params1=params,
                       params2=params )
        if ip1 is not None:
            self.intf1.setIP(ip1)

        if ip2 is not None:
            self.intf2.setIP(ip2)

class MyRouter( Node ):
    "A Node with routing."
    def config( self, **params ):
        super( MyRouter, self).config( **params )
        self.cmd( 'sysctl net.ipv4.ip_forward=1' )

class MultiHost( Host ):
    "A Node wiht two interfaces"
    def config( self, **params ):
        super( MultiHost, self).config( **params )
        self.cmd("ip rule add from 10.0.0.1 table 1")
        self.cmd("ip route add 10.0.0.0/24 dev h1-eth0 scope link table 1")
        self.cmd("ip route add default via 10.0.0.10 dev h1-eth0 scope link table 1")

        self.cmd("ip rule add from 10.0.0.2 table 2")
        self.cmd("ip route add 10.0.0.0/24 dev h1-eth1 scope link table 2")
        self.cmd("ip route add default via 10.0.0.10 dev h1-eth1 scope link table 2")


class MultipathTopo( Topo ):
    "A simple topology of a client with two interfaces connected to a node."

    def sysctl_set(self,key, value):
     """Issue systcl for given param to given value and check for error."""

     p = Popen("sysctl -w %s=%s" % (key, value), shell=True, stdout=PIPE, stderr=PIPE)
     # Output should be empty; otherwise, we have an issue. 
     stdout, stderr = p.communicate()
     stdout_expected = "%s = %s\n" % (key, value)
     if stdout != stdout_expected:
      raise Exception("Popen returned unexpected stdout: %s != %s" % (stdout, stdout_expected))
     if stderr:
      raise Exception("Popen returned unexpected stderr: %s" % stderr)


    def build( self, **opts ):

        s1 = self.addSwitch('sw1')
       
        host = self.addHost( 'h1', ip='10.0.0.1/24', cls=MultiHost)

        server = self.addNode( 's1', ip='10.0.0.10/24', cls=Node)



        linkConfig_wifi = {'bw': 20, 'delay': '10ms', 'loss': 0, 'jitter': 0, 'max_queue_size': 10000, 'txo': False, 'rxo': False }
        linkConfig_lte = {'bw': 50, 'delay': '10ms', 'loss': 0, 'jitter': 0, 'max_queue_size': 10000, 'txo': False, 'rxo': False }
        linkConfig_server = {'bw': 50, 'delay': '5ms', 'loss': 0, 'jitter': 0, 'max_queue_size': 10000, 'txo': False, 'rxo': False }

        # client connections
        self.addLink( host, s1, cls=MyTCLink, intfName1='h1-eth1', ip1='10.0.0.1/24',  **linkConfig_lte)
        self.addLink( host, s1, cls=MyTCLink, intfName1='h1-eth2', ip1='10.0.0.2/24', **linkConfig_wifi)    

        # server connections
        self.addLink( s1, server, cls=MyTCLink, **linkConfig_server)


if __name__ == '__main__':
    net = Mininet(topo=MultipathTopo(), switch=OVSBridge)

    net.start()
    CLI(net)
    net.stop()