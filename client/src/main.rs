use std::sync::Arc;

use decoder::{
    network::{LVNetwork, LVPacket},
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
    Logger::try_with_str("debug, wgpu=info")?.start()?;

    LVStatisticsCollector::start();

    // Bind value
    match std::env::args().nth(1) {
        Some(addr) => {
            let db = Arc::new(DoubleBuffer::new_uninitialized());
            let db_ui = db.clone();

            // Set up mpsc
            let (pkt_push, pkt_recv) = thingbuf::mpsc::blocking::channel::<LVPacket>(100);

            let receiver = LVNetwork::new(&addr);
            let decoder = LVDecoder::new();

            receiver.run(pkt_push);
            decoder.run(db, pkt_recv);

            // Start ui
            let ui = VideoUI::new()?;
            ui.run(db_ui).block_on()?;
        }
        None => println!("Usage: ./client addr"),
    }

    LVStatisticsCollector::quit();

    Ok(())
}
