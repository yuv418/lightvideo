use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use log::{debug, error, info};
use net::feedback_packet::LVFeedbackPacket;

pub struct LVFeedbackServer {
    bind_addr: String,
}

impl LVFeedbackServer {
    pub fn new(bind_addr: &str) -> Self {
        Self {
            bind_addr: bind_addr.to_owned(),
        }
    }

    fn handle_feedback(mut stream: TcpStream, bitrate_mtx: Arc<Mutex<u32>>) {
        let mut msg_buffer = vec![0; LVFeedbackPacket::no_bytes()];
        let mut bitrate = 10000;

        loop {
            match stream.read(&mut msg_buffer[..]) {
                Ok(data_read) => {
                    let feedback_packet: &LVFeedbackPacket = bytemuck::from_bytes(&msg_buffer[..]);
                    debug!("Feedback packet is {:?}", feedback_packet);

                    // Algorithm: we have the following information:
                    // - time quantum
                    // - total blocks
                    // - out of order blocks
                    // - total_packets (not using this yet)
                    // - lost_packets (not using this yet)
                    // - total RS decoder failures

                    // congestion = [(out of order blocks)/(total blocks)]

                    // bitrate =
                    //   (bitrate + 200) if congestion > 0.2
                    //   (bitrate) if 0.15 < congestion > 0.2 -- stable
                    //   (bitrate * 0.5) if (congestion < 0.15) or (decoder_failures > 0)

                    let congestion = feedback_packet.out_of_order_blocks as f32
                        / feedback_packet.total_blocks as f32;

                    {
                        let mut bitrate_mtx_set =
                            bitrate_mtx.lock().expect("Failed to lock bitrate mutex");

                        *bitrate_mtx_set = {
                            if congestion > 0.2 {
                                // this multiplication is not just integer division
                                // in case we want to change the multiplication constant
                                // later
                                (bitrate as f32 * 0.5) as u32
                            } else if congestion < 0.2 && congestion > 0.15 {
                                bitrate
                            } else {
                                bitrate + 200
                            }
                        };

                        // So we don't have to lock the mutex unnecessarily
                        bitrate = *bitrate_mtx_set;
                    }
                }
                Err(e) => error!("Could not read bytes from client {:?}", e),
            }
        }
    }

    pub fn start_receive_loop(
        bind_addr: &str,
        bitrate_shared: Arc<Mutex<u32>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(bind_addr)?;
        info!("feedback server is listening on {}", bind_addr);
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => Self::handle_feedback(stream, bitrate_shared.clone()),
                Err(e) => {
                    error!(
                        "Could not accept incoming feedback stream from client: {:?}",
                        e
                    )
                }
            }
        }

        Ok(())
    }

    pub fn begin(&self) -> Arc<Mutex<u32>> {
        info!("Starting feedback server");
        let bitrate_shared = Arc::new(Mutex::new(10000));
        let bitrate_shared_clone = bitrate_shared.clone();
        let bind_addr_clone = self.bind_addr.clone();
        thread::spawn(move || {
            Self::start_receive_loop(&bind_addr_clone, bitrate_shared_clone)
                .expect("Failed to start feedback server");
        });
        bitrate_shared
    }
}
