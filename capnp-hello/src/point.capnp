@0xc3e83e6ef265090f;

struct Point {
    x @0 : Float32;
    y @1 : Float32;
}

interface PointTracker {
    addPoint @0 (p: Point) -> (totalPoints : UInt64);
}