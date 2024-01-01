use flexi_logger::Logger;
use log::{error, info};

mod decoder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Logger::try_with_str("debug")?.start()?;

    // Bind value
    match std::env::args().nth(1) {
        Some(addr) => {
            let t = std::thread::spawn(move || match decoder::decode(&addr) {
                Ok(_) => info!("decoder exited ok"),
                Err(e) => error!("decoder exited with error {:?}", e),
            });

            let _ = t.join();
        }
        None => println!("Usage: ./client addr"),
    }
    Ok(())
}
