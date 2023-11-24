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
        Some("server") => {
            let server = Server::new("127.0.0.1:28771", 60, 1, 1920, 1080, 100000)?;
            server.begin()
        }
        _ => {
            println!("Usage: ./server {{bench|server}}");
            Ok(())
        }
    }
}
