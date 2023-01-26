@0xade63008e31e9022;

struct Data{
    bestRtt @0: UInt64;
    secondRtt @1: UInt64;
}

interface Scheduler{
    nextPath @0 (d :Data) -> (path:UInt8);
}