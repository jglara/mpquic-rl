import capnp
capnp.remove_import_hook()
point_capnp = capnp.load("./mpquic-quiche/src/data.capnp")

class SchedulerImpl(point_capnp.Scheduler.Server):
    def __init__(self):
        self.rtts = []

    def nextPath(self, d, _context, **kwargs):
        self.rtts.append((d.bestRtt, d.secondRtt))



        return 0

if __name__ == '__main__':
    addr= "0.0.0.0:6677"

    server = capnp.TwoPartyServer(addr, bootstrap=SchedulerImpl())
    server.run_forever()
