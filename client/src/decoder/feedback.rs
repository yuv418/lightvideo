use std::{io::Write, net::TcpStream, sync::Arc};

use log::{debug, error};
use net::feedback_packet::LVFeedbackPacket;
use parking_lot::Mutex;

const QUANTUM: u16 = 1000;

pub fn start(
    feedback_addr: &str,
) -> Result<Arc<Mutex<LVFeedbackPacket>>, Box<dyn std::error::Error>> {
    let feedback_packet: Arc<Mutex<LVFeedbackPacket>> = Arc::new(Mutex::new(Default::default()));
    let feedback_packet_clone = feedback_packet.clone();
    let feedback_timer = timer::Timer::new();
    let mut feedback_stream = TcpStream::connect(feedback_addr)?;

    feedback_timer.schedule_repeating(chrono::Duration::milliseconds(QUANTUM.into()), move || {
        debug!("writing feedback packet to server");
        let mut pkt = feedback_packet.lock();
        match feedback_stream.write(bytemuck::bytes_of(&*pkt)) {
            Ok(bytes) => debug!("wrote {} bytes to feedback server", bytes),
            Err(e) => error!("failed to send feedbacket packet with error {:?}", e),
        }

        // reset the feedback packet
        pkt.time_quantum = QUANTUM;
        pkt.total_blocks = 0;
        pkt.out_of_order_blocks = 0;
        pkt.total_packets = 0;
        pkt.lost_packets = 0;
        pkt.ecc_decoder_failures = 0;
    });

    Ok(feedback_packet_clone)
}
