from mininet.topo import Topo
from mininet.cli import CLI
from mininet.net import Mininet
from mininet.node import OVSBridge, Host
from mininet.link import Link
from mininet.link import TCIntf


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
        self.cmd("cd {quichepath};  RUST_LOG=info ./target/debug/mp_server --listen 10.0.0.10:4433 --cert ./apps/src/bin/cert.crt --key ./apps/src/bin/cert.key  --root {wwwpath} > /tmp/server.log&".format(
            quichepath='/home/ejogarv/DEV/quiche', 
            wwwpath='/home/ejogarv/DEV/mpquic-rl/mininet/static'))


class MultipathTopo( Topo ):
    def build(self, **opts):
        s1 = self.addSwitch('sw1')
        host = self.addHost( 'h1', ip='10.0.0.1/24', cls=MultiHost)
        server = self.addHost( 's1', ip='10.0.0.10/24', cls=QuicheQuicServer)


        linkConfig_lte = {'bw': 5, 'delay': '50ms', 'loss': 0, 'jitter': 0, 'max_queue_size': 10000 }
        linkConfig_wifi = {'bw': 5, 'delay': '5ms', 'loss': 0, 'jitter': 0, 'max_queue_size': 10000 }
        #linkConfig_server = {'bw': 50, 'delay': '5ms', 'loss': 0, 'jitter': 0, 'max_queue_size': 10000 }
#, 'txo': False, 'rxo': False


        # client connections
        self.addLink( s1, host, cls=MyTCLink, intfName2='h1-eth1', ip2='10.0.0.1/24', **linkConfig_lte)
        self.addLink( s1, host, cls=MyTCLink, intfName2='h1-eth2', ip2='10.0.0.2/24', **linkConfig_wifi)    
        
        

        # server connections
        self.addLink( s1, server)


if __name__ == '__main__':
    net = Mininet(topo=MultipathTopo(), switch=OVSBridge, controller=None)
    net.addNAT().configDefault()
    net.start()

 
    CLI(net)
    net.stop()
