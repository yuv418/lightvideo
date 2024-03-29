use flexi_logger::Logger;
use server::Server;
use statistics::collector::LVStatisticsCollector;

mod benchmark;
mod capture;
mod encoder;
mod packager;
mod server;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Logger::try_with_str("trace,statistics=debug")?.start()?;
    let quit_rx = LVStatisticsCollector::start();

    match std::env::args().nth(1).as_deref() {
        Some("bench") => benchmark::bench(),
        Some("server") => match std::env::args().nth(2) {
            Some(addr) => {
                let target_addr = std::env::args().nth(3).unwrap();
                let server = Server::new(&addr, &target_addr, 60, 0, 1920, 1080, 5000000, quit_rx)?;
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
