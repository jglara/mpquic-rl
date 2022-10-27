#[macro_use]
extern crate log;

use clap::{Parser};
//use quiche::PathStats;
use ring::rand::*;

use std::collections::HashMap;
use std::net::{self, SocketAddr};


#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct ServerCli {
    #[clap(value_parser, long, short)]
    listen: String, //Listen on the given IP:port [default: 127.0.0.1:4433]

    #[clap(value_parser, long, short)]
    cert: String, // TLS certificate path [default: src/bin/cert.crt]

    #[clap(value_parser, long, short)]
    key: String, //TLS certificate key path [default: src/bin/cert.key]

    #[clap(value_parser, long, short)]
    root: String, // path of root server

    #[clap(value_parser, long, short)]
    scheduler: String, // Type of scheduler

    #[clap(value_parser, long, short)]
    path_stats_output: String, // File to output path stats

    #[clap(value_parser, long)]
    conn_stats_output: String, // File to output conn stats

    #[clap(value_parser, long)]
    sched_stats_output: String, // File to output scheduler stats
}

const MAX_BUF_SIZE: usize = 65507;
const MAX_DATAGRAM_SIZE: usize = 1350;
struct PartialResponse {
    body: Vec<u8>,

    written: usize,
}
struct Client {
    conn: quiche::Connection,
    partial_responses: HashMap<u64, PartialResponse>,
    loss_rate: f64,
    max_send_burst: usize,
}

#[derive(Debug, serde::Serialize)]
struct PathStatsRecord<'a> {
    elapsed: u128,
    local: &'a SocketAddr,
    remote: &'a SocketAddr,
    sent_bytes: u64,
    recv_bytes: u64,
    cwnd: usize,
    bif: usize,
    rtt: u128,
    rttvar: u128,
    quantum: usize,
}

#[derive(Debug, serde::Serialize)]
struct ConnStatsRecord<'a> {
    elapsed: u128,
    trace_id: &'a str,
    max_send_burst: usize,
    sent_bytes: usize,
    sent_bytes_total: u64,
    lost_bytes_total: u64,
    retrans_bytes_total: u64,
    tx_cap: usize,
    max_tx_data: u64,
    stream_written: usize,
    pending: usize,
    max_off: u64,
    off_back: u64,
}

#[derive(Debug, serde::Serialize)]
struct ECFSchedulerStats {
    elapsed: u128,
    count: usize,
    waiting: bool,
    best_path_cwnd: usize,
    second_path_cwnd: usize,
    best_path_rtt: usize,
    second_path_rtt: usize,
    best_path_blocked: bool,
    send_on_second_path: bool,
    term1: usize,
    term2: usize,
    term3: usize,
    term4: usize,
    best_path_peer_addr: u16,
    second_path_peer_addr: u16,
}


trait Scheduler {
    fn start(&mut self, conn: &quiche::Connection);

    /// Return the next path
    fn next_path(&mut self, conn: &quiche::Connection) -> Option<(std::net::SocketAddr, std::net::SocketAddr)>;
}

struct RoundRobinScheduler{
    next: usize,
}


impl Scheduler for RoundRobinScheduler {
    fn start(&mut self, conn: &quiche::Connection) {
        self.next = conn.path_stats().count();
    }

    fn next_path(&mut self, conn: &quiche::Connection) -> Option<(std::net::SocketAddr, std::net::SocketAddr)> {
        
        self.next = if self.next >= conn.path_stats().count() {
            0
        } else {
            self.next + 1
        };
        //conn.path_stats().cycle().filter(|p| p.active && p.bytes_in_flight < p.cwnd).nth(self.next).map(|p| (p.local_addr, p.peer_addr))
        conn.path_stats().nth(self.next).map(|p| (p.local_addr, p.peer_addr))
        
    }
}

struct MinRttScheduler {

}

impl Scheduler for MinRttScheduler {
    fn start(&mut self, _conn: &quiche::Connection) {
        
    }

    fn next_path(&mut self, conn: &quiche::Connection) -> Option<(std::net::SocketAddr, std::net::SocketAddr)> {
        // always try each path at least once
        if let Some(p) = conn.path_stats().find(|p|p.sent < 3) {
            Some((p.local_addr, p.peer_addr))
        } else {
            conn.path_stats().filter(|p| p.active && p.bytes_in_flight < p.cwnd).min_by(|p1, p2| p1.rtt.cmp(&p2.rtt) )
            .map(|p| (p.local_addr, p.peer_addr))
        }
    }
}



struct ECFScheduler {
    waiting: bool,
    wrt: csv::Writer<std::fs::File>,
    server_start: std::time::Instant,
}

impl Scheduler for ECFScheduler {
    fn start(&mut self, conn: &quiche::Connection) {
        // calculate bestpath and secondpath with cwd,rtt,rttvar 
        //conn.path_stats().for_each(|p| debug!("{:?} -> {:?}: cwd: {} srtt: {:?} rttvar: {:?} q: {}", p.local_addr, p.peer_addr, p.cwnd, p.rtt, p.rttvar, conn.send_quantum_on_path(p.local_addr, p.peer_addr)));
        //conn.writable().filter_map(|id| conn.stream_send_offset(id).ok()).for_each(|(max_off, off_back)| debug!("Pending to send: {}", max_off - off_back));

      /*  self.wrt.serialize(ECFSchedulerStats {
            elapsed: self.server_start.elapsed().as_millis(),
            count: 0,
            waiting: self.waiting,
            best_path_blocked: false,
            send_on_second_path: false,
            term1: 0,
            term2: 0,
            term3: 0,
            term4: 0,
            best_path_rtt: 0,
            second_path_rtt: 0,
            best_path_cwnd: 0,
            second_path_cwnd: 0,
    }
    ).unwrap();*/
        
    }

    fn next_path(&mut self, conn: &quiche::Connection) -> Option<(std::net::SocketAddr, std::net::SocketAddr)> {
        // select bestpath, secondpath or nothing if waiting for bestpath is better

        let count = conn.path_stats().count();
        let mut best_path_blocked = false;
        let mut send_on_second_path = false;
        let mut term1 = 0;
        let mut term2 = 0;
        let mut term3 = 0;
        let mut term4 = 0;
        let mut best_rtt = 0;
        let mut second_rtt = 0;
        let mut best_path_cwnd =0;
        let mut second_path_cwnd = 0;
        let mut best_path_peer_addr= 0;
        let mut second_path_peer_addr= 0;


        let path = if let Some(p) = conn.path_stats().find(|p|p.sent < 3) {
            Some((p.local_addr, p.peer_addr))
        } else {
            let best_path = conn.path_stats().filter(|p| p.active).min_by(|p1, p2| p1.rtt.cmp(&p2.rtt) ).unwrap();
            let second_path = conn.path_stats().filter(|p| p.active).max_by(|p1, p2| p1.rtt.cmp(&p2.rtt) ).unwrap();
            best_path_peer_addr = best_path.peer_addr.port();
            second_path_peer_addr = second_path.peer_addr.port();

            let send_bytes: usize = conn.writable().filter_map(|id| conn.stream_send_offset(id).ok()).map(|(max_off, off_back)| max_off - off_back).sum::<u64>() as usize;
            let send_bytes_best = if send_bytes < best_path.cwnd {best_path.cwnd} else {send_bytes};
            let send_bytes_second = if send_bytes < second_path.cwnd {second_path.cwnd} else {send_bytes};
  
            let maxrttvar = std::cmp::max(best_path.rttvar.as_millis(), second_path.rttvar.as_millis()) as usize;
            best_rtt = best_path.rtt.as_millis() as usize;
            second_rtt = second_path.rtt.as_millis() as usize;
            best_path_cwnd = best_path.cwnd;
            second_path_cwnd = second_path.cwnd;


            if best_path.cwnd > best_path.bytes_in_flight {
                Some((best_path.local_addr, best_path.peer_addr))
            } else {
                best_path_blocked = true;

                
                term1 = 4 * (send_bytes_best + best_path_cwnd) * best_rtt;
                term2 = best_path_cwnd * (second_rtt + maxrttvar) * if self.waiting {5} else {4};

                if term1 < term2 {
                    term3 = send_bytes_second * second_rtt;
                    term4 = second_path_cwnd * ((2 * best_rtt) + maxrttvar) ;

                    if term3 > term4 {
                        self.waiting = true;
                        None
                    } else {
                        send_on_second_path= true;
                        Some((second_path.local_addr, second_path.peer_addr))
                    }
                } else {
                    self.waiting = false;
                    send_on_second_path= true;
                    Some((second_path.local_addr, second_path.peer_addr))
                }
            }
        };

        self.wrt.serialize(ECFSchedulerStats {
            elapsed: self.server_start.elapsed().as_millis(),
            count: count,
            waiting: self.waiting,
            best_path_blocked: best_path_blocked,
            send_on_second_path: send_on_second_path,
            term1: term1,
            term2: term2,
            term3: term3,
            term4: term4,
            best_path_rtt: best_rtt,
            second_path_rtt: second_rtt,
            best_path_cwnd: best_path_cwnd,
            second_path_cwnd: second_path_cwnd,
            best_path_peer_addr: best_path_peer_addr,
            second_path_peer_addr: second_path_peer_addr,
            }).unwrap();

        path

        
    }
}

struct BLESTScheduler {

}

impl Scheduler for BLESTScheduler {
    fn start(&mut self, conn: &quiche::Connection) {
        // calculate bestpath and secondpath with cwd,rtt and stream available sendbuffer
        todo!()
    }

    fn next_path(&mut self, conn: &quiche::Connection) -> Option<(std::net::SocketAddr, std::net::SocketAddr)> {
        // select bestpath, secondpath or nothing if waiting for bestpath is better to avoid HoL
        todo!()
    }
}

fn main() {
    env_logger::init();
    let cli = ServerCli::parse();


    info!("starting up server in {:?} with cert {} and key {}", cli.listen, cli.cert, cli.key);

    // Setup the event loop.
    let mut poll = mio::Poll::new().unwrap();
    let mut events = mio::Events::with_capacity(1024);
    let mut buf = [0; 65535];
    let mut out = [0; MAX_DATAGRAM_SIZE];

    let mut path_stats_wrt = csv::Writer::from_path(cli.path_stats_output).unwrap();
    let mut conn_stats_wrt = csv::Writer::from_path(cli.conn_stats_output).unwrap();
    let server_start = std::time::Instant::now();

    // Create the UDP listening socket, and register it with the event loop.
    let mut socket =
        mio::net::UdpSocket::bind(cli.listen.parse().unwrap()).unwrap();

    poll.registry()
        .register(&mut socket, mio::Token(0), mio::Interest::READABLE)
        .unwrap();

    // Create the configuration for the QUIC connections.
    let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION).unwrap();

    config
        .load_cert_chain_from_pem_file(&cli.cert)
        .unwrap();
    config
        .load_priv_key_from_pem_file(&cli.key)
        .unwrap();

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
    config.set_initial_max_stream_data_uni(1_000_000);
    config.set_initial_max_streams_bidi(100);
    config.set_initial_max_streams_uni(100);
    config.set_disable_active_migration(true);
    config.enable_early_data();
    config.set_multipath(true);


    let rng = SystemRandom::new();
    let conn_id_seed =
        ring::hmac::Key::generate(ring::hmac::HMAC_SHA256, &rng).unwrap();

    let local_addr = socket.local_addr().unwrap();
    let mut oclient: Option<Client> = None;
    let mut continue_write = false;
    let mut tx_cap = 0;

    let mut sched: Box<dyn Scheduler> = match cli.scheduler.as_str() {
        "rr" => Box::new(RoundRobinScheduler {next: 0}),
        "minRtt" => Box::new(MinRttScheduler {}),
        "blest" => Box::new(BLESTScheduler {}),
        "ecf" => Box::new(ECFScheduler {waiting: false, 
                                        wrt: csv::Writer::from_path(cli.sched_stats_output).unwrap(),
                                        server_start: server_start,
                                        }),
        _ => panic!("Invalid scheduler")
    };
    
    'main: loop {

        let timeout = if continue_write {
            Some(std::time::Duration::from_secs(0))            
        } else {
            oclient.as_ref().and_then(|c| c.conn.timeout())
        };

        poll.poll(&mut events, timeout).unwrap();

        // Read incoming UDP packets from the socket and feed them to quiche,
        // until there are no more packets to read.
        'read: loop {
            // If the event loop reported no events, it means that the timeout
            // has expired, so handle it without attempting to read packets. We
            // will then proceed with the send loop.
            if events.is_empty() && !continue_write{
                debug!("timed out");
                oclient.iter_mut().for_each(|c| c.conn.on_timeout());
 
                break 'read;
            }

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

            let pkt_buf = &mut buf[..len];

            // Parse the QUIC packet's header.
            let hdr = match quiche::Header::from_slice(
                pkt_buf,
                quiche::MAX_CONN_ID_LEN,
            ) {
                Ok(v) => v,

                Err(e) => {
                    error!("Parsing packet header failed: {:?}", e);
                    continue 'read;
                },
            };

            trace!("got packet {:?}", hdr);

            let conn_id = ring::hmac::sign(&conn_id_seed, &hdr.dcid);
            let conn_id = &conn_id.as_ref()[..quiche::MAX_CONN_ID_LEN];
            let conn_id: quiche::ConnectionId = conn_id.to_vec().into();

            let client = if oclient.is_none() {

                if hdr.ty != quiche::Type::Initial {
                    error!("Packet is not Initial");
                    continue 'read;
                }

                if !quiche::version_is_supported(hdr.version) {
                    warn!("Doing version negotiation");

                    let len =
                        quiche::negotiate_version(&hdr.scid, &hdr.dcid, &mut out)
                            .unwrap();

                    let out = &out[..len];

                    if let Err(e) = socket.send_to(out, from) {
                        if e.kind() == std::io::ErrorKind::WouldBlock {
                            debug!("send() would block");
                            break;
                        }

                        panic!("send() failed: {:?}", e);
                    }
                    continue 'read;
                }

                let mut scid = [0; quiche::MAX_CONN_ID_LEN];
                scid.copy_from_slice(&conn_id);

                let scid = quiche::ConnectionId::from_ref(&scid);

                // Token is always present in Initial packets.
                let token = hdr.token.as_ref().unwrap();

                // Do stateless retry if the client didn't send a token.
                if token.is_empty() {
                    warn!("Doing stateless retry");

                    let new_token = mint_token(&hdr, &from);

                    let len = quiche::retry(
                        &hdr.scid,
                        &hdr.dcid,
                        &scid,
                        &new_token,
                        hdr.version,
                        &mut out,
                    )
                    .unwrap();

                    let out = &out[..len];

                    if let Err(e) = socket.send_to(out, from) {
                        if e.kind() == std::io::ErrorKind::WouldBlock {
                            debug!("send() would block");
                            break;
                        }

                        panic!("send() failed: {:?}", e);
                    }
                    continue 'read;
                }

                let odcid = validate_token(&from, token);

                // The token was not valid, meaning the retry failed, so
                // drop the packet.
                if odcid.is_none() {
                    error!("Invalid address validation token");
                    continue 'read;
                }

                if scid.len() != hdr.dcid.len() {
                    error!("Invalid destination connection ID");
                    continue 'read;
                }

                // Reuse the source connection ID we sent in the Retry packet,
                // instead of changing it again.
                let scid = hdr.dcid.clone();

                debug!("New connection: dcid={:?} scid={:?}", hdr.dcid, scid);

                let mut conn = quiche::accept(
                    &scid,
                    odcid.as_ref(),
                    local_addr,
                    from,
                    &mut config,
                )
                .unwrap();
                

                if let Some(dir) = std::env::var_os("QLOGDIR") {
                    let id = format!("{:?}", &scid);
                    let writer = make_qlog_writer(&dir, "server", &id);

                    conn.set_qlog(
                        std::boxed::Box::new(writer),
                        "quiche-server qlog".to_string(),
                        format!("{} id={}", "quiche-server qlog", id),
                    );
                }

                oclient.get_or_insert(Client { 
                    conn, 
                    partial_responses: HashMap::new(),
                    loss_rate: 0.0,
                    max_send_burst: MAX_BUF_SIZE * 75 / 100,})
            } else {
                debug!("Incoming packet for connection dcid={:?} ", hdr.dcid);  
                oclient.as_mut().unwrap()
            };


            let recv_info = quiche::RecvInfo {
                to: socket.local_addr().unwrap(),
                from,
            };

            // Process potentially coalesced packets.
            let read = match client.conn.recv(pkt_buf, recv_info) {
                Ok(v) => v,

                Err(e) => {
                    error!("{} recv failed: {:?}", client.conn.trace_id(), e);
                    continue 'read;
                },
            };

            debug!("{} processed {} bytes", client.conn.trace_id(), read);
            if client.conn.is_in_early_data() || client.conn.is_established() {

                // Process all readable streams.
                for s in client.conn.readable() {
                    while let Ok((read, fin)) =
                        client.conn.stream_recv(s, &mut buf)
                    {
                        debug!(
                            "{} received {} bytes",
                            client.conn.trace_id(),
                            read
                        );

                        let stream_buf = &buf[..read];

                        debug!(
                            "{} stream {} has {} bytes (fin? {})",
                            client.conn.trace_id(),
                            s,
                            stream_buf.len(),
                            fin
                        );

                        handle_stream(client, s, stream_buf, &cli.root);
                    }
                }
            }

            handle_path_events(client);

            // See whether source Connection IDs have been retired.
            while let Some(retired_scid) = client.conn.retired_scid_next() {
                info!("Retiring source CID {:?}", retired_scid);
            }

            // Provides as many CIDs as possible.
            while client.conn.source_cids_left() > 0 {
                let (scid, reset_token) = generate_cid_and_reset_token(&rng);
                if client
                    .conn
                    .new_source_cid(&scid, reset_token, false)
                    .is_err()
                {
                    break;
                }

                info!("Adding new source CID {:?}", scid);
            }

        } //read loop

        //debug!("{} done reading", client.conn.trace_id());

        continue_write = false;
        // Generate outgoing QUIC packets for all active connections and send
        // them on the UDP socket, until quiche reports that there are no more
        // packets to be sent.
        if let Some(client) = oclient.as_mut() {

            tx_cap = client.conn.tx_cap;
            let mut max_off =0;
            let mut off_back= 0;
            // Handle writable streams.
            
            for stream_id in client.conn.writable() {
                    //info!("cap {:?} {:?} {:?}", client.conn.tx_cap, client.conn.tx_data, client.conn.max_tx_data);
                    handle_writable(client, stream_id);
                    if let Ok((max_off_, off_back_)) = client.conn.stream_send_offset(stream_id) {
                        max_off = max_off_;
                        off_back = off_back_;
                    }
            }    

            let max_datagram_size = client.conn.max_send_udp_payload_size();
            // Reduce max_send_burst by 25% if loss is increasing more than 0.1%.
            let loss_rate =
                client.conn.stats().lost as f64 / client.conn.stats().sent as f64;
            if loss_rate > client.loss_rate + 0.001 {
                client.max_send_burst = client.max_send_burst / 4 * 3;
                // Minimun bound of 10xMSS.
                client.max_send_burst =
                    client.max_send_burst.max(max_datagram_size * 10);
                client.loss_rate = loss_rate;
            }

            let max_send_burst = /*client.max_send_burst / max_datagram_size * max_datagram_size;*/
                 client.conn.send_quantum().min(client.max_send_burst) /
                    max_datagram_size *
                    max_datagram_size;
            let mut total_write = 0;
            
            sched.start(&client.conn);
            
            'write: while total_write < max_send_burst {                
                if let Some( (local_addr, peer_addr) ) = sched.next_path(&client.conn) {                
                    let (write, send_info) = match client.conn.send_on_path(&mut out, Some(local_addr), Some(peer_addr)) {
                        Ok(v) => v,

                        Err(quiche::Error::Done) => {
                            debug!("{} done writing", client.conn.trace_id());
                            break 'write;
                        },

                        Err(e) => {
                            error!("{} send failed: {:?}", client.conn.trace_id(), e);

                            client.conn.close(false, 0x1, b"fail").ok();
                            break 'write;
                        },
                    };

                    total_write += write;

                    if let Err(e) = socket.send_to(&out[..write], send_info.to) {
                        if e.kind() == std::io::ErrorKind::WouldBlock {
                            debug!("send() would block");
                            continue 'write;
                        }

                        panic!("send() failed: {:?}", e);
                    }

                    debug!("{}: {} -> {} written {} bytes", client.conn.trace_id(), local_addr, send_info.to, write);
                    
                } else {
                    debug!("No path to send");
                    break 'write;
                }

                if total_write >= max_send_burst {
                    debug!("{} pause writing", client.conn.trace_id(),);
                    continue_write = true;
                }
            }

            //info!("Sending over paths: {:?}", scheduled_tuples);
            client.conn.path_stats().for_each(|p| {
                let (srtt,rttvar) = if p.rtt.as_millis() == 333 {(0,0)} else {(p.rtt.as_millis(), p.rttvar.as_millis())};

                
                path_stats_wrt.serialize(PathStatsRecord {
                    elapsed: server_start.elapsed().as_millis(),
                    local: &p.local_addr, 
                    remote: &p.peer_addr, 
                    sent_bytes: p.sent_bytes, 
                    recv_bytes: p.recv_bytes, 
                    cwnd: p.cwnd, 
                    bif: p.bytes_in_flight, 
                    rtt: srtt,
                    rttvar: rttvar,
                    quantum: client.conn.send_quantum_on_path(p.local_addr, p.peer_addr),
                }).unwrap()
            }); 

            //info!("cap {:?} {:?} {:?}", client.conn.tx_cap, client.conn.tx_data, client.conn.max_tx_data);
            conn_stats_wrt.serialize( ConnStatsRecord {
                elapsed: server_start.elapsed().as_millis(),
                trace_id: client.conn.trace_id(),
                max_send_burst: max_send_burst,
                sent_bytes: total_write,
                tx_cap: tx_cap,
                max_off: max_off,
                off_back: off_back,
                max_tx_data: client.conn.max_tx_data,
                sent_bytes_total: client.conn.stats().sent_bytes,
                lost_bytes_total: client.conn.stats().lost_bytes,
                retrans_bytes_total: client.conn.stats().stream_retrans_bytes,
                stream_written: client.partial_responses.iter().map(|(_, r)| r.written).sum(),
                pending: client.partial_responses.iter().map(|(_, r)| r.body.len() - r.written).sum(),
            }).unwrap();




            if client.conn.is_closed() {
                    println!(
                        "{} connection closed {:?}",
                        client.conn.trace_id(),
                        client.conn.stats()
                    );

                    break 'main;
    
            }
        }


    }


}



/// Generate a stateless retry token.
///
/// The token includes the static string `"quiche"` followed by the IP address
/// of the client and by the original destination connection ID generated by the
/// client.
///
/// Note that this function is only an example and doesn't do any cryptographic
/// authenticate of the token. *It should not be used in production system*.
fn mint_token(hdr: &quiche::Header, src: &net::SocketAddr) -> Vec<u8> {
    let mut token = Vec::new();

    token.extend_from_slice(b"quiche");

    let addr = match src.ip() {
        std::net::IpAddr::V4(a) => a.octets().to_vec(),
        std::net::IpAddr::V6(a) => a.octets().to_vec(),
    };

    token.extend_from_slice(&addr);
    token.extend_from_slice(&hdr.dcid);

    token
}


/// Validates a stateless retry token.
///
/// This checks that the ticket includes the `"quiche"` static string, and that
/// the client IP address matches the address stored in the ticket.
///
/// Note that this function is only an example and doesn't do any cryptographic
/// authenticate of the token. *It should not be used in production system*.
fn validate_token<'a>(
    src: &net::SocketAddr, token: &'a [u8],
) -> Option<quiche::ConnectionId<'a>> {
    if token.len() < 6 {
        return None;
    }

    if &token[..6] != b"quiche" {
        return None;
    }

    let token = &token[6..];

    let addr = match src.ip() {
        std::net::IpAddr::V4(a) => a.octets().to_vec(),
        std::net::IpAddr::V6(a) => a.octets().to_vec(),
    };

    if token.len() < addr.len() || &token[..addr.len()] != addr.as_slice() {
        return None;
    }

    Some(quiche::ConnectionId::from_ref(&token[addr.len()..]))
}



/// Handles incoming HTTP/0.9 requests.
fn handle_stream(client: &mut Client, stream_id: u64, buf: &[u8], root: &str) -> usize {
    let conn = &mut client.conn;

    if buf.len() > 4 && &buf[..4] == b"GET " {
        let uri = &buf[4..buf.len()];
        let uri = String::from_utf8(uri.to_vec()).unwrap();
        let uri = String::from(uri.lines().next().unwrap());
        let uri = std::path::Path::new(&uri);
        let mut path = std::path::PathBuf::from(root);

        for c in uri.components() {
            if let std::path::Component::Normal(v) = c {
                path.push(v)
            }
        }

        info!(
            "{} got GET request for {:?} on stream {}",
            conn.trace_id(),
            path,
            stream_id
        );

        let body = std::fs::read(path.as_path())
            .unwrap_or_else(|_| b"Not Found!\r\n".to_vec());

        info!(
            "{} sending response of size {} on stream {}",
            conn.trace_id(),
            body.len(),
            stream_id
        );

        let written = match conn.stream_send(stream_id, &body, true) {
            Ok(v) => v,

            Err(quiche::Error::Done) => 0,

            Err(e) => {
                error!("{} stream send failed {:?}", conn.trace_id(), e);
                return 0;
            },
        };

        if written < body.len() {
            let response = PartialResponse { body, written };
            client.partial_responses.insert(stream_id, response);
        }
        return written;
    }

    return 0;
}


/// Handles newly writable streams.
fn handle_writable(client: &mut Client, stream_id: u64) -> usize {
    let conn = &mut client.conn;

    debug!("{} stream {} is writable", conn.trace_id(), stream_id);

    if !client.partial_responses.contains_key(&stream_id) {
        debug!("{} stream with no partial responses", stream_id);
        return 0;
    }

    let resp = client.partial_responses.get_mut(&stream_id).unwrap();
    let body = &resp.body[resp.written..];

    let written = match conn.stream_send(stream_id, body, true) {
        Ok(v) => v,

        Err(quiche::Error::Done) => 0,

        Err(e) => {
            client.partial_responses.remove(&stream_id);

            error!("{} stream send failed {:?}", conn.trace_id(), e);
            return 0;
        },
    };
    
    resp.written += written;
    debug!("Wrote {} bytes. pending {}", written, resp.body.len() - resp.written);

    if resp.written == resp.body.len() {
        client.partial_responses.remove(&stream_id);
    }

    written
}


fn handle_path_events(client: &mut Client) {
    while let Some(qe) = client.conn.path_event_next() {
        info!("Path event {:?}", qe);
        match qe {
            quiche::PathEvent::New(local_addr, peer_addr) => {
                info!(
                    "{} Seen new path ({}, {})",
                    client.conn.trace_id(),
                    local_addr,
                    peer_addr
                );

                // Directly probe the new path.
                client
                    .conn
                    .probe_path(local_addr, peer_addr)
                    .expect("cannot probe");
            },

            quiche::PathEvent::Validated(local_addr, peer_addr) => {
                info!(
                    "{} Path ({}, {}) is now validated",
                    client.conn.trace_id(),
                    local_addr,
                    peer_addr
                );
                client.conn.set_active(local_addr, peer_addr, true).ok();
            },

            quiche::PathEvent::FailedValidation(local_addr, peer_addr) => {
                info!(
                    "{} Path ({}, {}) failed validation",
                    client.conn.trace_id(),
                    local_addr,
                    peer_addr
                );
            },

            quiche::PathEvent::Closed(local_addr, peer_addr, err, reason) => {
                info!(
                    "{} Path ({}, {}) is now closed and unusable; err = {} reason = {:?}",
                    client.conn.trace_id(),
                    local_addr,
                    peer_addr,
                    err,
                    reason,
                );
            },

            quiche::PathEvent::ReusedSourceConnectionId(cid_seq, old, new) => {
                info!(
                    "{} Peer reused cid seq {} (initially {:?}) on {:?}",
                    client.conn.trace_id(),
                    cid_seq,
                    old,
                    new
                );
            },

            quiche::PathEvent::PeerMigrated(local_addr, peer_addr) => {
                info!(
                    "{} Connection migrated to ({}, {})",
                    client.conn.trace_id(),
                    local_addr,
                    peer_addr
                );
            },

            quiche::PathEvent::PeerPathStatus(addr, path_status) => {
                info!("Peer asks status {:?} for {:?}", path_status, addr,);
                client
                    .conn
                    .set_path_status(addr.0, addr.1, path_status, false)
                    .expect("cannot follow status request");
            },
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
