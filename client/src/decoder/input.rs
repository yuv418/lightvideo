// TODO: we need to move some of this to a different directory

use std::{
    net::{SocketAddrV4, UdpSocket},
    thread,
};

use log::{debug, error};
use net::input::{input_packet_size, LVInputEvent, LVInputEventType};

pub fn start(
    bind_addr: SocketAddrV4,
    event_recv: flume::Receiver<LVInputEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let input_sock = UdpSocket::bind(bind_addr)?;

    // Stupid way to establish a bi-directional communication channel
    thread::spawn(move || {
        // DIY buffered reader that doesn't write too much.
        let mut inp_buffer = vec![0; input_packet_size()];
        debug!("input: waiting for ping from server on {:?}", bind_addr);
        match input_sock.recv_from(&mut inp_buffer[..]) {
            Ok((_, send_addr)) => {
                debug!("send_addr is {:?}", send_addr);

                loop {
                    match event_recv.recv() {
                        Ok(ev) => {
                            debug!("sending event {:?}", ev);
                            inp_buffer[0] = match ev {
                                LVInputEvent::KeyboardEvent(ke) => {
                                    inp_buffer[1..].copy_from_slice(bytemuck::bytes_of(&ke));
                                    LVInputEventType::KeyboardEvent
                                }
                                LVInputEvent::MouseClickEvent(mce) => {
                                    inp_buffer[1..].copy_from_slice(bytemuck::bytes_of(&mce));
                                    LVInputEventType::MouseClickEvent
                                }
                                LVInputEvent::MouseWheelEvent(mwe) => {
                                    inp_buffer[1..].copy_from_slice(bytemuck::bytes_of(&mwe));
                                    LVInputEventType::MouseWheelEvent
                                }
                                LVInputEvent::MouseMoveEvent(mme) => {
                                    inp_buffer[1..].copy_from_slice(bytemuck::bytes_of(&mme));
                                    LVInputEventType::MouseMoveEvent
                                }
                            } as u8;
                        }
                        Err(e) => error!("Did not receive input packet from flume {:?}", e),
                    }

                    match input_sock.send_to(&inp_buffer, send_addr) {
                        Ok(n) => debug!("sent {} bytes to input server", n),
                        Err(e) => error!("did not send input to input server {:?}", e),
                    }
                }
            }
            Err(e) => error!("Failed to receive ping from server"),
        }
    });
    Ok(())
}
