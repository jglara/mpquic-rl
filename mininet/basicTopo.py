from mininet.topo import Topo
from mininet.cli import CLI
from mininet.net import Mininet
from mininet.node import OVSBridge, Host
from mininet.link import Link
from mininet.link import TCIntf
from mininet.log import setLogLevel, info, debug

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


class Router( Host ):
    "A Node with forwarding on"
    def config( self, **params ):
        super( Router, self).config( **params )
        self.cmd("sysctl -w net.ipv4.ip_forward=1")


class MultiHost( Host ):
    "A Node wiht two interfaces"
    def config( self, **params ):
        super( MultiHost, self).config( **params )
        self.cmd("ip rule add from 10.0.1.1 table 1")
        self.cmd("ip route add 10.0.1.0/24 dev h1-eth1 scope link table 1")
        self.cmd("ip route add default via 10.0.1.10 dev h1-eth1 table 1")

        self.cmd("ip rule add from 10.0.2.1 table 2")
        self.cmd("ip route add 10.0.2.0/24 dev h1-eth2 scope link table 2")
        self.cmd("ip route add default via 10.0.2.10 dev h1-eth2  table 2")

class PicoQuicServer( Host ):
    "quic server"
    def config( self, **params ):
        super(PicoQuicServer, self).config( **params )
        self.cmd("cd {picopath}; ./picoquicdemo -M 2 -c ./certs/cert.pem -k ./certs/key.pem -p 4433 -w ./static/ &".format(
            picopath='/home/ejogarv/DEV/picoquic', 
            wwwpath='/home/ejogarv/DEV/mpquic-rl/mininet/static'))


class QuicheQuicServer( Host ):
    "quic server"
    def config( self, **params ):
        super(QuicheQuicServer, self).config( **params )
        self.cmd("cd {quichepath};  RUST_LOG=info ./target/debug/mp_server --listen 10.0.3.10:4433 --cert ./apps/src/bin/cert.crt --key ./apps/src/bin/cert.key  --root {wwwpath} > /tmp/server.log&".format(
            quichepath='/home/ejogarv/DEV/quiche', 
            wwwpath='/home/ejogarv/DEV/mpquic-rl/mininet/static'))


class MultipathTopo( Topo ):
    def build(self, **opts):
        sw1 = self.addSwitch("sw1")
        sw2 = self.addSwitch("sw2")
        sw3 = self.addSwitch("sw3")
        host = self.addHost( 'h1', ip='10.0.1.1/24', cls=MultiHost)
        server = self.addHost( 's1', ip='10.0.3.10/24', cls=QuicheQuicServer, defaultRoute='via 10.0.3.1')
        router = self.addHost( 'r1', ip='10.0.3.1/24', cls=Router)


        linkConfig_lte = {'bw': 50, 'delay': '5ms', 'loss': 0, 'jitter': 0, 'max_queue_size': 10000 }
        linkConfig_wifi = {'bw': 10, 'delay': '15ms', 'loss': 0, 'jitter': 0, 'max_queue_size': 10000 }
        #linkConfig_server = {'bw': 50, 'delay': '5ms', 'loss': 0, 'jitter': 0, 'max_queue_size': 10000 }
#, 'txo': False, 'rxo': False

        # server router connections
        self.addLink( sw3, server, cls=MyTCLink, intfName2='s1-eth1', ip2='10.0.3.10/24')
        self.addLink( sw3, router, cls=MyTCLink, intfName2='r1-eth3', ip2='10.0.3.1/24')

        # client router connections
        self.addLink( sw1, host, cls=MyTCLink, intfName2='h1-eth1', ip2='10.0.1.1/24', **linkConfig_lte)
        self.addLink( sw1, router, cls=MyTCLink, intfName2='r1-eth1', ip2='10.0.1.10/24')

        self.addLink( sw2, host, cls=MyTCLink, intfName2='h1-eth2', ip2='10.0.2.1/24', **linkConfig_wifi)
        self.addLink( sw2, router, cls=MyTCLink, intfName2='r1-eth2', ip2='10.0.2.10/24')    
        

if __name__ == '__main__':
    setLogLevel('info')
    topo=MultipathTopo()
    net = Mininet(topo, switch=OVSBridge, controller=None)
    #net.addNAT().configDefault()
    net.start()

 
    CLI(net)
    net.stop()
