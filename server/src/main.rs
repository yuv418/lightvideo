use flexi_logger::Logger;
use server::Server;

mod benchmark;
mod capture;
mod encoder;
mod packager;
mod server;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Logger::try_with_str("debug")?.start()?;

    match std::env::args().nth(1).as_deref() {
        Some("bench") => benchmark::bench(),
        Some("server") => match std::env::args().nth(2) {
            Some(addr) => {
                let target_addr = std::env::args().nth(3).unwrap();
                let server = Server::new(&addr, &target_addr, 60, 0, 1920, 1080, 7000000)?;
                server.begin()
            }
            None => {
                println!("Usage: ./server {{bench|server}} bind_addr target_addr");
                Ok(())
            }
        },
        _ => {
            println!("Usage: ./server {{bench|server}} (options)");
            Ok(())
        }
    }
}
