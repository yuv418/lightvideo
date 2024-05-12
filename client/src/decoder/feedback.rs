use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use log::{debug, error};
use net::feedback_packet::{LVAck, LVFeedbackPacket, ACK_TYPE, FEEDBACK_TYPE};
use parking_lot::Mutex;

const QUANTUM: u16 = 1000;

pub fn start(
    feedback_addr: &str,
    feedback_pkt: Arc<Mutex<(LVAck, LVFeedbackPacket)>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let feedback_addr = feedback_addr.to_owned();
    let x = TcpListener::bind(feedback_addr)?;
    thread::spawn(move || {
        let feedback_timer = timer::Timer::new();

        debug!("connecting to feedback server...");

        for feedback_stream in x.incoming() {
            match feedback_stream {
                Ok(mut feedback_stream) => {
                    debug!("connected to feedback server");
                    loop {
                        {
                            debug!("writing feedback packet to server");
                            let mut pkt = feedback_pkt.lock();
                            debug!("feebdback packet is {:?}", pkt);

                            // Copying *rolls eyes*
                            let mut data: Vec<u8> = bytemuck::bytes_of(&pkt.1).to_owned();
                            data.insert(0, FEEDBACK_TYPE);

                            match feedback_stream.write(&data) {
                                Ok(bytes) => debug!("wrote {} bytes to feedback server", bytes),
                                Err(e) => {
                                    error!("failed to send feedbacket packet with error {:?}", e)
                                }
                            }

                            // reset the feedback packet
                            pkt.1.time_quantum = QUANTUM;
                            pkt.1.total_blocks = 0;
                            pkt.1.out_of_order_blocks = 0;
                            pkt.1.total_packets = 0;
                            pkt.1.lost_packets = 0;
                            pkt.1.ecc_decoder_failures = 0;

                            // send the ACK packet
                            pkt.0.send_ts = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis();

                            let mut data: Vec<u8> = bytemuck::bytes_of(&pkt.0).to_owned();
                            data.insert(0, ACK_TYPE);
                            match feedback_stream.write(&data) {
                                Ok(bytes) => debug!("wrote {} ack bytes to feedback server", bytes),
                                Err(e) => {
                                    error!("failed to send ack packet with error {:?}", e)
                                }
                            }
                            // no need to reset the ACK as we just set it the next time.
                        }

                        thread::sleep(std::time::Duration::from_millis(QUANTUM.into()));
                    }
                }
                Err(e) => error!("Failed to unwrap feedback stream! {:?}", e),
            }
        }
    });

    Ok(())
}
