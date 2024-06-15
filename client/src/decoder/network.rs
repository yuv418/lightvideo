use std::{
    borrow::BorrowMut,
    io::Write,
    net::{SocketAddrV4, TcpStream, UdpSocket},
    os::fd::{AsRawFd, RawFd},
    sync::Arc,
    thread,
    time::Instant,
};

use bytes::BytesMut;
use log::{debug, error, info};
use net::{
    feedback_packet::{LVAck, LVFeedbackPacket},
    input::LVInputEvent,
};
use parking_lot::{Mutex, RwLock};
use socket2::Socket;
use thingbuf::mpsc::{blocking::Sender, errors::Closed};

use crate::decoder::input;

use super::feedback;

const MTU_SIZE: usize = 1200;

pub struct LVNetwork {
    addr: String,
}

#[derive(Clone)]
pub struct LVPacketHolder {
    pub payload: BytesMut,
    pub amt: usize,
}

impl Default for LVPacketHolder {
    fn default() -> Self {
        Self {
            payload: {
                let mut bm = BytesMut::new();
                bm.resize(MTU_SIZE, 0);
                bm
            },
            amt: 0,
        }
    }
}

impl LVNetwork {
    pub fn new(addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Feedback address is addr + 2
        Ok(Self {
            addr: addr.to_owned(),
        })
    }

    pub fn run(
        &self,
        packet_push: Sender<LVPacketHolder>,
        inp_recv: flume::Receiver<LVInputEvent>,
        feedback_pkt: Arc<Mutex<(LVAck, LVFeedbackPacket)>>,
        udp_fd: Arc<RwLock<Option<RawFd>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let addr = self.addr.clone();

        thread::Builder::new()
            .name("network_thread".to_string())
            .spawn(move || {
                if let Err(e) =
                    Self::socket_loop(packet_push, inp_recv, feedback_pkt, &addr, udp_fd)
                {
                    error!("socket receive loop failed with error {:?}", e);
                } else {
                    info!("socket receive loop exited.");
                }
            });

        Ok(())
    }

    fn socket_loop(
        packet_push: Sender<LVPacketHolder>,
        inp_recv: flume::Receiver<LVInputEvent>,
        feedback_pkt: Arc<Mutex<(LVAck, LVFeedbackPacket)>>,
        addr: &str,
        udp_fd: Arc<RwLock<Option<RawFd>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sock = UdpSocket::bind(addr)?;
        let sock = Socket::from(sock);

        debug!("current recv size {:?}", sock.recv_buffer_size());
        sock.set_recv_buffer_size(393216)?;
        debug!("new recv size {:?}", sock.recv_buffer_size());

        let sock: UdpSocket = sock.into();
        *udp_fd.write() = Some(sock.as_raw_fd());

        debug!("starting thread for socket, listening on {}", addr);

        let mut feedback_addr: SocketAddrV4 = addr.parse()?;
        feedback_addr.set_port(feedback_addr.port() + 2);

        debug!("initializing feedback server to {:?}", feedback_addr);

        // TODO: don't fail so loudly.
        feedback::start(&feedback_addr.to_string(), feedback_pkt.clone())?;

        // Input setup
        let mut input_bind_addr = feedback_addr;
        input_bind_addr.set_port(input_bind_addr.port() + 1);

        debug!("initializing input server to {:?}", input_bind_addr);

        input::start(input_bind_addr, inp_recv);

        loop {
            // By using the packet push, we avoid allocating on the heap every iteration.
            match packet_push.try_send_ref() {
                Ok(mut data_ref) => {
                    let (amt, src) = sock.recv_from(&mut data_ref.payload)?;
                    data_ref.amt = amt;

                    debug!("recv received {} bytes from {}", amt, src);
                }
                Err(e) => error!("thingbuf try_send_ref returns {:?}", e),
            }
        }
    }
}
