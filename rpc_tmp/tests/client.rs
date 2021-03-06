extern crate mqtt;
extern crate mqtt3;
extern crate net;
extern crate pi_lib;
extern crate rpc;

use rpc::client::RPCClient;
use rpc::traits::RPCClientTraits;

use std::io::Result;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::thread;

use mqtt::client::ClientNode;
use mqtt::data::Client;
use mqtt3::{LastWill, QoS};
use pi_lib::atom::Atom;

use std::thread::sleep;
use std::time::Duration;


use net::{Config, NetManager, Protocol, RawSocket, RawStream};

fn handle_close(stream_id: usize, reason: Result<()>) {
    println!(
        "client handle_close, stream_id = {}, reson = {:?}",
        stream_id, reason
    );
}

fn client_request(mut rpc: RPCClient) {
    sleep(Duration::from_secs(2));
    rpc.request(
        Atom::from("a/b/c"),
        String::from("hello world").into_bytes(),
        Box::new(move |r: Result<Arc<Vec<u8>>>| {
            let r = &*(r.unwrap());
            println!("request !!!!!!!!!result: {:?}", String::from_utf8(r.clone()).unwrap());
        }),
        100,
    )
}

fn handle_connect(peer: Result<(RawSocket, Arc<RwLock<RawStream>>)>, addr: Result<SocketAddr>) {
    let (socket, stream) = peer.unwrap();
    println!(
        "client handle_connect: addr = {:?}, socket:{}",
        addr.unwrap(),
        socket.socket
    );
    {
        let stream = &mut stream.write().unwrap();

        stream.set_close_callback(Box::new(|id, reason| handle_close(id, reason)));
        stream.set_send_buf_size(1024 * 1024);
        stream.set_recv_timeout(500 * 1000);
    }

    let mut client_node = ClientNode::new();
    client_node.set_stream(socket, stream);

    let mut rpc = RPCClient::new(client_node);
    //遗言
    let last_will = LastWill {
        topic: String::from("$last_will"),
        message: String::from("{clientid:1, msg:'xxx'}"),
        qos: QoS::AtMostOnce,
        retain: false,
    };
    rpc.connect(
        10,
        Some(last_will),
        Some(Box::new(|_r| println!("client handle_close ok "))),
        Some(Box::new(|_r| {
            println!("client connect ok!!!!!!!!!");
        })),
    );
    thread::spawn(move || {
        client_request(rpc)
        });
}

pub fn start_client() -> NetManager {
    let mgr = NetManager::new();
    let config = Config {
        protocol: Protocol::TCP,
        addr: "127.0.0.1:1234".parse().unwrap(),
    };
    mgr.connect(config, Box::new(|peer, addr| handle_connect(peer, addr)));

    return mgr;
}
