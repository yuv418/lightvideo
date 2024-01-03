use std::sync::Arc;

use decoder::LVDecoder;
use double_buffer::DoubleBuffer;
use flexi_logger::Logger;
use log::{error, info};
use ui::VideoUI;

use pollster::FutureExt as _;

mod decoder;
mod double_buffer;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Logger::try_with_str("info, wgpu=info")?.start()?;

    // Bind value
    match std::env::args().nth(1) {
        Some(addr) => {
            let db = Arc::new(DoubleBuffer::new_uninitialized());
            let db_ui = db.clone();

            let t = std::thread::spawn(move || {
                let mut decoder = LVDecoder::new(db);
                match decoder.decode(&addr) {
                    Ok(_) => info!("decoder exited ok"),
                    Err(e) => error!("decoder exited with error {:?}", e),
                }
            });

            // Start ui
            let ui = VideoUI::new()?;
            ui.run(db_ui).block_on()?;
        }
        None => println!("Usage: ./client addr"),
    }
    Ok(())
}
