use std::{net::SocketAddrV4, sync::Arc};

use decoder::{
    feedback,
    network::{LVNetwork, LVPacketHolder},
    video::LVDecoder,
};
use double_buffer::DoubleBuffer;
use flexi_logger::Logger;
use log::{error, info};
use statistics::collector::LVStatisticsCollector;
use ui::VideoUI;

use pollster::FutureExt as _;

mod decoder;
mod double_buffer;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Logger::try_with_str("trace, calloop=info, wgpu=info")?.start()?;

    let quit_rx = LVStatisticsCollector::start();

    // Bind value
    match std::env::args().nth(1) {
        Some(addr) => {
            let db = Arc::new(DoubleBuffer::new_uninitialized());
            let db_ui = db.clone();

            // Set up mpsc
            let (pkt_push, pkt_recv) = thingbuf::mpsc::blocking::channel::<LVPacketHolder>(1000);

            let mut feedback_addr: SocketAddrV4 = addr.parse()?;
            feedback_addr.set_port(feedback_addr.port() + 2);

            let feedback_pkt = feedback::start(&feedback_addr.to_string())?;

            let receiver = LVNetwork::new(&addr)?;

            receiver.run(pkt_push);
            LVDecoder::run(db, pkt_recv, feedback_pkt);

            // Start ui
            let ui = VideoUI::new(quit_rx)?;
            ui.run(db_ui).block_on()?;
        }
        None => println!("Usage: ./client addr"),
    }

    LVStatisticsCollector::quit();

    Ok(())
}
