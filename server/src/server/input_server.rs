use std::{net::UdpSocket, thread};

use log::{debug, error, info};
use net::input::{input_packet_size, LVInputEvent, LVInputEventType};

use crate::input::LVInputEmulator;

pub struct LVInputServer {
    bind_addr: String,
}

impl LVInputServer {
    pub fn new(bind_addr: &str) -> Self {
        Self {
            bind_addr: bind_addr.to_owned(),
        }
    }

    pub fn start_receive_loop(
        &self,
        mut input_emulator: Box<dyn LVInputEmulator>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind(&self.bind_addr).expect("Failed to make socket");
        info!("binding input server at {}", self.bind_addr);

        // The server pads the input data
        thread::spawn(move || {
            let mut buf = vec![0; input_packet_size()];
            loop {
                // By using the packet push, we avoid allocating on the heap every iteration.
                match socket.recv_from(&mut buf) {
                    Ok(n) => {
                        debug!("received {} input bytes from client", n.0);

                        // Determine variant and construct input event
                        match LVInputEventType::try_from(buf[0]) {
                            Ok(input_variant) => {
                                let input_event = match input_variant {
                                    LVInputEventType::KeyboardEvent => LVInputEvent::KeyboardEvent(
                                        *bytemuck::from_bytes(&buf[1..]),
                                    ),
                                    LVInputEventType::MouseClickEvent => {
                                        LVInputEvent::MouseClickEvent(*bytemuck::from_bytes(
                                            &buf[1..],
                                        ))
                                    }
                                    LVInputEventType::MouseWheelEvent => {
                                        LVInputEvent::MouseWheelEvent(*bytemuck::from_bytes(
                                            &buf[1..],
                                        ))
                                    }
                                    LVInputEventType::MouseMoveEvent => {
                                        LVInputEvent::MouseMoveEvent(*bytemuck::from_bytes(
                                            &buf[1..],
                                        ))
                                    }
                                };

                                debug!("Received input event {:?}", input_event);

                                input_emulator.write_event(input_event);
                            }
                            Err(e) => {
                                error!("Failed to get LVInputEventType from input data: {:?}", e)
                            }
                        }
                    }
                    Err(e) => {
                        error!("failed to recieve input bytes from client {:?}", e)
                    }
                }
            }
        });
        Ok(())
    }
}
