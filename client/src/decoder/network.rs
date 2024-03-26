use std::{
    io::Write,
    net::{SocketAddrV4, TcpStream, UdpSocket},
    thread,
    time::Instant,
};

use bytes::BytesMut;
use log::{debug, error, info};
use net::feedback_packet::LVFeedbackPacket;
use socket2::Socket;
use thingbuf::mpsc::{blocking::Sender, errors::Closed};

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
    ) -> Result<(), Box<dyn std::error::Error>> {
        let addr = self.addr.clone();

        thread::Builder::new()
            .name("network_thread".to_string())
            .spawn(move || {
                if let Err(e) = Self::socket_loop(packet_push, &addr) {
                    error!("socket receive loop failed with error {:?}", e);
                } else {
                    info!("socket receive loop exited.");
                }
            });

        Ok(())
    }

    fn socket_loop(
        packet_push: Sender<LVPacketHolder>,
        addr: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sock = UdpSocket::bind(addr)?;
        let sock = Socket::from(sock);

        debug!("current recv size {:?}", sock.recv_buffer_size());
        sock.set_recv_buffer_size(393216)?;
        debug!("new recv size {:?}", sock.recv_buffer_size());

        let sock: UdpSocket = sock.into();
        debug!("starting thread for socket, listening on {}", addr);

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
