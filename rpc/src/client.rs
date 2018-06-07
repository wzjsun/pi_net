use std::io::{Result};
/**
 * RPC传输协议：
 * 消息体：1字节表示压缩和版本,4字节消息ID，1字节超时时长（0表示不超时), 剩下的BonBuffer ,
 * 第一字节：前3位表示压缩算法，后5位表示版本（灰度）
 * 压缩算法：0：不压缩，1：rsync, 2:LZ4 BLOCK, 3:LZ4 SEREAM, 4、5、6、7预留
 */
use std::sync::{Arc, RwLock};

use fnv::FnvHashMap;
use pi_lib::atom::Atom;

use mqtt3;
use mqtt3::{LastWill};

use mqtt::client::ClientNode;
use mqtt::data::{Client, ClientCallback};
use mqtt::session::{LZ4_BLOCK, UNCOMPRESS};
use mqtt::util;

use net::Socket;

use pi_base::util::{compress, uncompress, CompressLevel};
use traits::RPCClientTraits;

pub struct RPCClient {
    mqtt: ClientNode,
    msg_id: u32,
    handlers: Arc<RwLock<FnvHashMap<u32, Box<Fn(Arc<Vec<u8>>)>>>>,
}

impl RPCClient {
    pub fn new(mqtt: ClientNode) -> Self {
        RPCClient {
            mqtt,
            msg_id: 0,
            handlers: Arc::new(RwLock::new(FnvHashMap::default())),
        }
    }
    pub fn connect(
        &mut self,
        keep_alive: u16,
        will: Option<LastWill>,
        close_func: Option<ClientCallback>,
        connect_func: Option<ClientCallback>,
    ) {
        //连接MQTTser
        self.mqtt
            .connect(keep_alive, will, close_func, connect_func);
        let handlers = self.handlers.clone();
        //topic回调方法
        let topic_handle = move |r: Result<(Socket, &[u8])>| {
            let (socket, data) = r.unwrap();
            let header = data[0];
            //压缩版本
            let compress = (&header >> 5) as u8;
            //消息版本
            let _vsn = &header & 0b11111;
            let msg_id = u32::from_be(unsafe { *((data[1..4].as_ptr()) as *mut u32) });
            let mut rdata = Vec::new();
            match compress {
                UNCOMPRESS => rdata.extend_from_slice(&data[5..]),
                LZ4_BLOCK => {
                    let mut vec_ = Vec::new();
                    uncompress(&data[6..], &mut vec_).is_ok();
                    rdata.extend_from_slice(&vec_[..]);
                }
                _ => socket.close(true),
            }
            let rdata = Arc::new(rdata);
            let mut handlers = handlers.write().unwrap();
            match handlers.get(&msg_id) {
                Some(func) => {
                    func(rdata);
                }
                None => socket.close(true),
            };
            handlers.remove(&msg_id);
        };
        self.mqtt.set_topic_handler(
            Atom::from(String::from("$r").as_str()),
            Box::new(move |r| topic_handle(r)),
        ).is_ok();
    }
}

impl RPCClientTraits for RPCClient {
    fn request(&mut self, func_name: Atom, msg: Vec<u8>, resp: Box<Fn(Arc<Vec<u8>>)>, timeout: u8) {
        self.msg_id += 1;
        let socket = self.mqtt.get_socket();
        let mut buff: Vec<u8> = vec![];
        let msg_size = msg.len();
        let msg_id = self.msg_id;
        let mut compress_vsn = UNCOMPRESS;
        let mut body = vec![];
        if msg_size > 64 {
            compress_vsn = LZ4_BLOCK;
            compress(msg.as_slice(), &mut body, CompressLevel::High).is_ok();
        } else {
            body = msg;
        }
        //第一字节：3位压缩版本、5位消息版本 TODO 消息版本以后定义
        buff.push(((compress_vsn << 5) | 0) as u8);
        let b1: u8 = ((msg_id >> 24) & 0xff) as u8;
        let b2: u8 = ((msg_id >> 16) & 0xff) as u8;
        let b3: u8 = ((msg_id >> 8) & 0xff) as u8;
        let b4: u8 = (msg_id & 0xff) as u8;
        //4字节消息ID
        buff.extend_from_slice(&[b1, b2, b3, b4]);
        //一字节超时时长（秒）
        buff.push(timeout);
        //剩下的消息体
        buff.extend_from_slice(body.as_slice());
        //发布消息
        util::send_publish(&socket, false, mqtt3::QoS::AtMostOnce, &func_name, buff);
        let mut handlers = self.handlers.write().unwrap();
        handlers.insert(msg_id, resp);
    }
}