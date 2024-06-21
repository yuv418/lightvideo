use std::{
    mem::size_of,
    net::{SocketAddr, UdpSocket},
    thread,
};

use log::{debug, error, info};
use net::input::{
    input_packet_size, LVInputEvent, LVInputEventType, LVKeyboardEvent, LVMouseClickEvent,
    LVMouseMoveEvent, LVMouseWheelEvent,
};

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
        input_target_addr: SocketAddr,
        mut input_emulator: Box<dyn LVInputEmulator>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("bind address is {:?}", self.bind_addr);
        debug!("input target address is {:?}", input_target_addr);
        let socket = UdpSocket::bind(&self.bind_addr).expect("Failed to make socket");

        // How do we guarantee this gets to the other side?
        for _ in 0..5 {
            debug!("sending ping to {:?}", input_target_addr);
            socket.send_to(&[1, 0, 0, 0], input_target_addr);
        }

        info!("binding input server at {}", self.bind_addr);

        // The server pads the input data
        thread::spawn(move || {
            let mut buf = vec![0; input_packet_size()];
            let max_align = net::input::max_align();
            loop {
                // By using the packet push, we avoid allocating on the heap every iteration.
                match socket.recv_from(&mut buf) {
                    Ok(n) => {
                        debug!("received {} input bytes from client", n.0);

                        // Determine variant and construct input event
                        match LVInputEventType::try_from(buf[0]) {
                            Ok(input_variant) => {
                                let input_event = match input_variant {
                                    LVInputEventType::KeyboardEvent => {
                                        LVInputEvent::KeyboardEvent(*bytemuck::from_bytes(
                                            &buf[max_align
                                                ..size_of::<LVKeyboardEvent>() + max_align],
                                        ))
                                    }
                                    LVInputEventType::MouseClickEvent => {
                                        LVInputEvent::MouseClickEvent(*bytemuck::from_bytes(
                                            &buf[max_align
                                                ..size_of::<LVMouseClickEvent>() + max_align],
                                        ))
                                    }
                                    LVInputEventType::MouseWheelEvent => {
                                        LVInputEvent::MouseWheelEvent(*bytemuck::from_bytes(
                                            &buf[max_align
                                                ..size_of::<LVMouseWheelEvent>() + max_align],
                                        ))
                                    }
                                    LVInputEventType::MouseMoveEvent => {
                                        debug!(
                                            "Mouse move event data is {:?}",
                                            &buf[max_align
                                                ..size_of::<LVMouseMoveEvent>() + max_align]
                                        );
                                        LVInputEvent::MouseMoveEvent(*bytemuck::from_bytes(
                                            &buf[max_align
                                                ..size_of::<LVMouseMoveEvent>() + max_align],
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
