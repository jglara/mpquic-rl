#[macro_use]
extern crate log;

use clap::Parser;
//use mio::net::UdpSocket;
use ring::rand::*;
use std::net::ToSocketAddrs;
use std::collections::HashMap;
use std::net::SocketAddr;


#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct ClientCli {
    #[clap(value_parser, long, short)]
    lte_addr: String, // first addr

    #[clap(value_parser, long, short)]
    wifi_addr: String, // alternative addr

    #[clap(value_parser, long, short)]
    url: String, // resource

    #[clap(value_parser, long, short)]
    download_stats_output: String, // File to output download stats

    #[clap(value_parser, long, short)]
    path_stats_output: String, // File to output client stats
}


#[derive(Debug, serde::Serialize)]
struct DownloadStats {
    recv_bytes: u64,
    elapsed: u128,
}

#[derive(Debug, serde::Serialize)]

struct PathStatsRecord<'a> {
    elapsed: u128,
    local: &'a SocketAddr,
    remote: &'a SocketAddr,
    recv_bytes: usize,
    off_front: u64,
    max_off: u64,
}


const MAX_DATAGRAM_SIZE: usize = 1350;

const HTTP_REQ_STREAM_ID: u64 = 4;


fn main() {

    env_logger::init();
    let cli = ClientCli::parse();

    let mut stats_wrt = csv::Writer::from_path(cli.download_stats_output).unwrap();
    let mut path_stats_wrt = csv::Writer::from_path(cli.path_stats_output).unwrap();

    let mut buf = [0; 65535];
    let mut out = [0; MAX_DATAGRAM_SIZE];

    let url = url::Url::parse(&cli.url).unwrap();

    // Setup the event loop.
    let mut poll = mio::Poll::new().unwrap();
    let mut events = mio::Events::with_capacity(1024);

    // Resolve server address.
    let peer_addr = url.to_socket_addrs().unwrap().next().unwrap();


    // Bind to INADDR_ANY or IN6ADDR_ANY depending on the IP family of the
    // server address. This is needed on macOS and BSD variants that don't
    // support binding to IN6ADDR_ANY for both v4 and v6.

    // Create the UDP socket backing the QUIC connection, and register it with
    // the event loop.
    let addrs = [cli.lte_addr, cli.wifi_addr];

    //let mut sockets: Vec<UdpSocket> = Vec::new();
    let mut src_addrs = HashMap::new();
    let mut sockets = Vec::new();

    // for path probes
    let mut probed_paths = 1;

    let mut tok=0;
    for src_addr in &addrs {
        info!("registeging source address {:?}", src_addr);
        let socket = mio::net::UdpSocket::bind(src_addr.parse().unwrap()).unwrap();
        let local_addr = socket.local_addr().unwrap();
        src_addrs.insert(local_addr, tok);
        sockets.push(socket);
        poll.registry()
            .register(
                &mut sockets[tok],
                mio::Token(tok),
                mio::Interest::READABLE,
            )
            .unwrap();
        tok +=1;
    }


    // Create the configuration for the QUIC connection.
    let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION).unwrap();

    // *CAUTION*: this should not be set to `false` in production!!!
    config.verify_peer(false);

    config
        .set_application_protos(&[
            b"hq-interop",
            b"hq-29",
            b"hq-28",
            b"hq-27",
            b"http/0.9",
        ])
        .unwrap();

    config.set_max_idle_timeout(5000);
    config.set_max_recv_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_max_send_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_initial_max_data(10_000_000);
    config.set_initial_max_stream_data_bidi_local(1_500_000);
    config.set_initial_max_stream_data_bidi_remote(1_500_000);
    config.set_initial_max_streams_bidi(100);
    config.set_initial_max_streams_uni(100);
    config.set_disable_active_migration(true);
    config.set_multipath(true);

    // Add keylogging support
    let mut keylog = None;
    if let Some(keylog_path) = std::env::var_os("SSLKEYLOGFILE") {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(keylog_path)
            .unwrap();

        keylog = Some(file);
        config.log_keys();
    }

    
  

    // Generate a random source connection ID for the connection.
    let mut scid = [0; quiche::MAX_CONN_ID_LEN];
    let rng = SystemRandom::new();
    rng.fill(&mut scid[..]).unwrap();

    let scid = quiche::ConnectionId::from_ref(&scid);

    // Get local address.
    let local_addr = sockets[0].local_addr().unwrap();

    // Create a QUIC connection and initiate handshake.
    let mut conn =
        quiche::connect(url.domain(), &scid, local_addr, peer_addr, &mut config)
            .unwrap();

    if let Some(keylog) = &mut keylog {
        if let Ok(keylog) = keylog.try_clone() {
            conn.set_keylog(Box::new(keylog));
        }
    }

    if let Some(dir) = std::env::var_os("QLOGDIR") {
        let id = format!("{:?}", scid);
        let writer = make_qlog_writer(&dir, "client", &id);

        conn.set_qlog(
                std::boxed::Box::new(writer),
                "quiche-client qlog".to_string(),
                format!("{} id={}", "quiche-client qlog", id),
          );
    }

    info!(
        "connecting to {:} from {:} with scid {}",
        peer_addr,
        local_addr,
        hex_dump(&scid)
    );

    let (write, send_info) = conn.send(&mut out).expect("initial send failed");

    while let Err(e) = sockets[0].send_to(&out[..write], send_info.to) {
        if e.kind() == std::io::ErrorKind::WouldBlock {
            debug!("send() would block");
            continue;
        }

        panic!("send() failed: {:?}", e);
    }

    debug!("written {}", write);

    let mut req_start = std::time::Instant::now();

    let mut req_sent = false;

    loop {
        poll.poll(&mut events, conn.timeout()).unwrap();

        if events.is_empty() {
            debug!("timed out");
            conn.on_timeout();
        }

        for event in &events {
            let token: usize = event.token().into();
            let socket = &sockets[token];
            let local_addr = socket.local_addr().unwrap();
            
            // Read incoming UDP packets from the socket and feed them to quiche,
            // until there are no more packets to read.
            'read: loop {
                // If the event loop reported no events, it means that the timeout
                // has expired, so handle it without attempting to read packets. We
                // will then proceed with the send loop.
                
                let (len, from) = match socket.recv_from(&mut buf) {
                    Ok(v) => v,
                    
                    Err(e) => {
                        // There are no more UDP packets to read, so end the read
                        // loop.
                        if e.kind() == std::io::ErrorKind::WouldBlock {
                            debug!("recv() would block");
                            break 'read;
                        }
                        
                        panic!("recv() failed: {:?}", e);
                    },
                };
                
                debug!("got {} bytes", len);
                
                let recv_info = quiche::RecvInfo {
                    to: socket.local_addr().unwrap(),
                    from,
                };
                
                // Process potentially coalesced packets.
                let read = match conn.recv(&mut buf[..len], recv_info) {
                    Ok(v) => v,
                    
                    Err(e) => {
                        error!("recv failed: {:?}", e);
                        continue 'read;
                    },
                };

                let mut max_off=0;
                let mut off_front= 0;
                if let Some((max_off_, off_front_)) = conn.readable().filter_map(|id| conn.stream_recv_offset(id).ok() ).next() {
                    max_off = max_off_;
                    off_front = off_front_;
                }

                path_stats_wrt.serialize(PathStatsRecord {
                    elapsed: req_start.elapsed().as_millis(),
                    local: &recv_info.to,
                    remote: &recv_info.from,
                    recv_bytes: read,
                    off_front: off_front,
                    max_off: max_off,
                }).unwrap();

                debug!("{} processed {} bytes", local_addr, read);
            }
        }

        debug!("done reading");

        if conn.is_closed() {
            info!("connection closed, {:?}", conn.stats());
            break;
        }

        // Send an HTTP request as soon as the connection is established.
        if conn.is_established() && !req_sent  && conn.path_stats().count() >= 2 {
            req_start = std::time::Instant::now();
            info!("sending HTTP request for {}", url.path());

            let req = format!("GET {}\r\n", url.path());
            conn.stream_send(HTTP_REQ_STREAM_ID, req.as_bytes(), true)
                .unwrap();

            req_sent = true;
        }

        // Process all readable streams.
        for s in conn.readable() {
            while let Ok((read, fin)) = conn.stream_recv(s, &mut buf) {
                debug!("received {} bytes", read);

                let stream_buf = &buf[..read];

                debug!(
                    "stream {} has {} bytes (fin? {})",
                    s,
                    stream_buf.len(),
                    fin
                );

                //print!("{}", unsafe {
                //    std::str::from_utf8_unchecked(stream_buf)
                //});

                // The server reported that it has no more data to send, which
                // we got the full response. Close the connection.
                if s == HTTP_REQ_STREAM_ID && fin {
                    println!(
                        "{:?} bytes received in {:?}, closing...", conn.stats().recv_bytes,
                        req_start.elapsed()
                    );
                    stats_wrt.serialize(
                        DownloadStats {
                            recv_bytes: conn.stats().recv_bytes, 
                            elapsed: req_start.elapsed().as_millis(),
                    }).unwrap();

                    conn.close(true, 0x00, b"kthxbye").unwrap();
                }
            }
        }

        // handle path events
        handle_path_events(&mut conn);

        // send path probes
        // See whether source Connection IDs have been retired.
        while let Some(retired_scid) = conn.retired_scid_next() {
            info!("Retiring source CID {:?}", retired_scid);
        }
        
        // Provides as many CIDs as possible.
        while conn.source_cids_left() > 0 {
            let (scid, reset_token) = generate_cid_and_reset_token(&rng);
            
            if conn.new_source_cid(&scid, reset_token, false).is_err() {
                break;
            }
            info!("Adding new source CID {:?}", scid);
        }
        //info!("ppaths={} dcids={}", probed_paths, conn.available_dcids());
        if probed_paths < sockets.len() &&
        conn.available_dcids() > 0 &&
        conn.probe_path(sockets[probed_paths].local_addr().unwrap(), peer_addr).is_ok()
        {

            info!("Probe path {} -> {}", sockets[probed_paths].local_addr().unwrap(), peer_addr);
            probed_paths += 1;
        }

        // Determine in which order we are going to iterate over paths.
        let scheduled_tuples = lowest_latency_scheduler(&conn);


        // Generate outgoing QUIC packets and send them on the UDP socket, until
        // quiche reports that there are no more packets to be sent.
        for (local_addr, peer_addr) in scheduled_tuples {
            let token = src_addrs[&local_addr];
            let socket = &sockets[token];

            loop {
                let (write, send_info) = match conn.send_on_path(&mut out, Some(local_addr), Some(peer_addr)) {
                    Ok(v) => v,

                    Err(quiche::Error::Done) => {
                        debug!("done writing");
                        break;
                    },

                    Err(e) => {
                        error!("send failed: {:?}", e);

                        conn.close(false, 0x1, b"fail").ok();
                        break;
                    },
                };

                if let Err(e) = socket.send_to(&out[..write], send_info.to) {
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        debug!("send() would block");
                        break;
                    }

                    panic!("send() failed: {:?}", e);
                }

                trace!("{} -> {}: written {}", local_addr, send_info.to, write);
            }
        }

        if conn.is_closed() {
            println!("connection closed, {:?}", conn.stats());
            break;
        }
    }
}

fn hex_dump(buf: &[u8]) -> String {
    let vec: Vec<String> = buf.iter().map(|b| format!("{:02x}", b)).collect();

    vec.join("")
}


fn handle_path_events(conn: &mut quiche::Connection)
{
    // Handle path events.
    while let Some(qe) = conn.path_event_next() {
        match qe {
            quiche::PathEvent::New(..) => unreachable!(),
            
            quiche::PathEvent::Validated(local_addr, peer_addr) => {
                info!(
                    "Path ({}, {}) is now validated",
                    local_addr, peer_addr
                );
                
                conn.set_active(local_addr, peer_addr, true).ok();
            },
            
            quiche::PathEvent::FailedValidation(local_addr, peer_addr) => {
                info!(
                    "Path ({}, {}) failed validation",
                    local_addr, peer_addr
                );
            },
            
            quiche::PathEvent::Closed(local_addr, peer_addr, e, reason) => {
                info!(
                    "Path ({}, {}) is now closed and unusable; err = {}, reason = {:?}",
                    local_addr, peer_addr, e, reason
                );
            },
            
            quiche::PathEvent::ReusedSourceConnectionId(
                cid_seq,
                old,
                new,
            ) => {
                info!(
                    "Peer reused cid seq {} (initially {:?}) on {:?}",
                    cid_seq, old, new
                );
            },
            
            quiche::PathEvent::PeerMigrated(..) => unreachable!(),
            
            quiche::PathEvent::PeerPathStatus(..) => {},
        }
    }
    

}


/// Generate a new pair of Source Connection ID and reset token.
fn generate_cid_and_reset_token<T: SecureRandom>(
    rng: &T,
) -> (quiche::ConnectionId<'static>, u128) {
    let mut scid = [0; quiche::MAX_CONN_ID_LEN];
    rng.fill(&mut scid).unwrap();
    let scid = scid.to_vec().into();
    let mut reset_token = [0; 16];
    rng.fill(&mut reset_token).unwrap();
    let reset_token = u128::from_be_bytes(reset_token);
    (scid, reset_token)
}


/// Generate a ordered list of 4-tuples on which the host should send packets,
/// following a lowest-latency scheduling.
fn lowest_latency_scheduler(
    conn: &quiche::Connection,
) -> impl Iterator<Item = (std::net::SocketAddr, std::net::SocketAddr)> {
    use itertools::Itertools;
    conn.path_stats()
        .sorted_by_key(|p| p.rtt)
        .map(|p| (p.local_addr, p.peer_addr))
}



/// Makes a buffered writer for a qlog.
pub fn make_qlog_writer(
    dir: &std::ffi::OsStr, role: &str, id: &str,
) -> std::io::BufWriter<std::fs::File> {
    let mut path = std::path::PathBuf::from(dir);
    let filename = format!("{}-{}.sqlog", role, id);
    path.push(filename);

    match std::fs::File::create(&path) {
        Ok(f) => std::io::BufWriter::new(f),

        Err(e) => panic!(
            "Error creating qlog file attempted path was {:?}: {}",
            path, e
        ),
    }
}