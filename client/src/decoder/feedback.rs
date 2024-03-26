use std::{io::Write, net::TcpStream, sync::Arc};

use log::{debug, error};
use net::feedback_packet::LVFeedbackPacket;
use parking_lot::Mutex;

pub fn start(
    feedback_addr: &str,
) -> Result<Arc<Mutex<LVFeedbackPacket>>, Box<dyn std::error::Error>> {
    let feedback_packet: Arc<Mutex<LVFeedbackPacket>> = Arc::new(Mutex::new(Default::default()));
    let feedback_packet_clone = feedback_packet.clone();
    let feedback_timer = timer::Timer::new();
    let mut feedback_stream = TcpStream::connect(feedback_addr)?;

    feedback_timer.schedule_repeating(chrono::Duration::seconds(1), move || {
        let pkt = feedback_packet.lock();
        match feedback_stream.write(bytemuck::bytes_of(&*pkt)) {
            Ok(bytes) => debug!("wrote {} bytes to feedback server", bytes),
            Err(e) => error!("failed to send feedbacket packet with error {:?}", e),
        }
    });

    Ok(feedback_packet_clone)
}
