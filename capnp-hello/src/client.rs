pub mod point_capnp {
    include!(concat!(env!("OUT_DIR"), "/point_capnp.rs"));
}

use futures::AsyncReadExt;
use std::net::{ToSocketAddrs, SocketAddr};
use capnp_rpc::{rpc_twoparty_capnp::{self, Side}, twoparty, RpcSystem};
use crate::point_capnp::{point, point_tracker};

use tokio::sync::mpsc;


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() != 2 {
        println!("usage: {} client HOST:PORT", args[0]);
        return Ok(());
    }

    let addr = args[1]
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("could not parse address");
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;

     // Set up a channel for communicating.
     let (send_req, mut recv_req) = mpsc::channel(16);
     let (send_resp, mut recv_resp) = mpsc::channel(16);

    std::thread::spawn(move || {
        rt.block_on(async move {
            let stream = tokio::net::TcpStream::connect(&addr).await.unwrap();

            println!("Connected to TCP Stream");

            stream.set_nodelay(true).unwrap();
            let (r, w) =
                tokio_util::compat::TokioAsyncReadCompatExt::compat(stream).split();

            let network = twoparty::VatNetwork::new(r,w, 
                    rpc_twoparty_capnp::Side::Client, Default::default());
    
            let mut rpc_system = RpcSystem::new(Box::new(network), None);
            let point_tracker: point_tracker::Client = rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);

            tokio::task::LocalSet::new().run_until( async {
                tokio::task::spawn_local(rpc_system);

                while let Some((x,y)) = recv_req.recv().await {
                   let mut request = point_tracker.add_point_request();

                   let mut msg = ::capnp::message::Builder::new_default();
                    let mut p = msg.init_root::<point::Builder>();
                    p.set_x(x);
                    p.set_y(y);

                   request.get().set_p(p.into_reader()).unwrap();
                
                    let reply = request.send().promise.await.unwrap();
                    
                    send_resp.send(reply.get().unwrap().get_total_points()).await.unwrap();
                }
            }).await

        });
    }
    );


    send_req.blocking_send((5_f32, 10_f32))?;

    if let Some(resp) = recv_resp.blocking_recv() {
        println!("Total points in Point Tracker: {}", resp );       
        
    }

    Ok(())
        
}