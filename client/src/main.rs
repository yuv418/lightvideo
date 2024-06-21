use std::{
    net::{SocketAddr, SocketAddrV4},
    os::fd::RawFd,
    sync::Arc,
};

use decoder::{
    feedback, input,
    network::{LVNetwork, LVPacketHolder},
    video::LVDecoder,
};
use double_buffer::DoubleBuffer;
use flexi_logger::Logger;
use log::{error, info};
use net::{
    feedback_packet::{LVAck, LVFeedbackPacket},
    input::LVInputEvent,
};
use parking_lot::{Mutex, RwLock};
use statistics::collector::LVStatisticsCollector;
use ui::VideoUI;

use pollster::FutureExt as _;

mod decoder;
mod double_buffer;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Logger::try_with_str("trace, calloop=info, wgpu=info, client::decoder::video=info, client::decoder::feedback=info, client::decoder::network=info, client::ui::wgpu_state=info, client::double_buffer=info")?.start()?;

    let quit_rx = LVStatisticsCollector::start();

    // Bind value
    match std::env::args().nth(1) {
        Some(addr) => {
            let db = Arc::new(DoubleBuffer::new_uninitialized());
            let db_ui = db.clone();

            // Set up mpsc
            let (pkt_push, pkt_recv) = thingbuf::mpsc::blocking::channel::<LVPacketHolder>(1000);
            let (inp_push, inp_recv) = flume::bounded::<LVInputEvent>(10);

            let feedback_pkt: Arc<Mutex<(LVAck, LVFeedbackPacket)>> =
                Arc::new(Mutex::new((Default::default(), Default::default())));

            let udp_fd: Arc<RwLock<Option<RawFd>>> = Arc::new(RwLock::new(None));

            let receiver = LVNetwork::new(&addr)?;

            receiver.run(pkt_push, inp_recv, feedback_pkt.clone(), udp_fd.clone())?;
            LVDecoder::run(db, pkt_recv, feedback_pkt.clone(), udp_fd);

            // Start ui
            let ui = VideoUI::new(quit_rx)?;
            ui.run(db_ui, inp_push).block_on()?;
        }
        None => println!("Usage: ./client addr"),
    }

    LVStatisticsCollector::quit();

    Ok(())
}
