import capnp
capnp.remove_import_hook()
point_capnp = capnp.load("./src/point.capnp")

class PointTrackerImpl(point_capnp.PointTracker.Server):
    def __init__(self):
        self.points = []

    def addPoint(self, p, _context, **kwargs):
        print("p.x = {} p.y = {}".format(p.x, p.y))
        self.points.append((p.x, p.y))
        return len(self.points)

if __name__ == '__main__':
    addr= "0.0.0.0:6677"

    server = capnp.TwoPartyServer(addr, bootstrap=PointTrackerImpl())
    server.run_forever()


