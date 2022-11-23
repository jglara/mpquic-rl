pub mod point_capnp {
    include!(concat!(env!("OUT_DIR"), "/point_capnp.rs"));
}

pub mod point_demo {
    use crate::point_capnp::point;
    use capnp::serialize_packed;
    use capnp::message::Builder;
    use std::io::stdout;

    pub fn write_to_stream() -> ::capnp::Result<()> {
        let mut message = Builder::new_default();
        let mut demo_point = message.init_root::<point::Builder>();

        demo_point.set_x(5_f32);
        demo_point.set_y(10_f32);

        serialize_packed::write_message(&mut stdout(), &message)

    }
}

use std::net::ToSocketAddrs;
use capnp::capability::Promise;
use capnp_rpc::{pry, rpc_twoparty_capnp, twoparty, RpcSystem};
use futures::{AsyncReadExt, FutureExt};
use crate::point_capnp::point_tracker;

pub struct Point {
    x: f32,
    y: f32,
}

struct PointTrackerImpl {
    points: Vec<Point>,
}


impl point_tracker::Server for PointTrackerImpl {
    fn add_point(&mut self,
        params:point_tracker::AddPointParams<>,
        mut results:point_tracker::AddPointResults<>) ->  Promise<(), capnp::Error> {

        let point_client = pry!(params.get()).get_p();

        if let Ok(p) = point_client {
            println!("Received p = {} {}",p.get_x(), p.get_y());
            self.points.push( Point {
                x: p.get_x(),
                y: p.get_y(),
            });

        }
        results.get().set_total_points(self.points.len() as u64);

        Promise::ok(())
    }   
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = ::std::env::args().collect();


    let addr = args[1].to_socket_addrs().unwrap().next().expect("Invalid addr");

    tokio::task::LocalSet::new().run_until(async move {
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        let point_tracker_client : point_tracker::Client = capnp_rpc::new_client(PointTrackerImpl {
            points: Vec::new(),
        });        

        loop {
            let (stream, _) = listener.accept().await?;
            stream.set_nodelay(true)?;

            let (r,w) = tokio_util::compat::TokioAsyncReadCompatExt::compat(stream).split();

            let network = twoparty::VatNetwork::new(r,w, 
                rpc_twoparty_capnp::Side::Server, Default::default());

            let rpc_system = RpcSystem::new(Box::new(network), Some(point_tracker_client.clone().client));

            tokio::task::spawn_local(rpc_system);

        }
    }).await

}