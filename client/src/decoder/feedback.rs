use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::Arc,
    thread,
};

use log::{debug, error};
use net::feedback_packet::LVFeedbackPacket;
use parking_lot::Mutex;

const QUANTUM: u16 = 1000;

pub fn start(
    feedback_addr: &str,
    feedback_pkt: Arc<Mutex<LVFeedbackPacket>>,
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
                            match feedback_stream.write(bytemuck::bytes_of(&*pkt)) {
                                Ok(bytes) => debug!("wrote {} bytes to feedback server", bytes),
                                Err(e) => {
                                    error!("failed to send feedbacket packet with error {:?}", e)
                                }
                            }

                            // reset the feedback packet
                            pkt.time_quantum = QUANTUM;
                            pkt.total_blocks = 0;
                            pkt.out_of_order_blocks = 0;
                            pkt.total_packets = 0;
                            pkt.lost_packets = 0;
                            pkt.ecc_decoder_failures = 0;
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
