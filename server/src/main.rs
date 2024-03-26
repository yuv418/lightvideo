use std::net::{Ipv4Addr, SocketAddr};

use flexi_logger::Logger;
use server::{feedback_server::LVFeedbackServer, streaming_server::LVStreamingServer};
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
                let mut feedback_addr: SocketAddr = target_addr.parse()?;

                feedback_addr.set_port(feedback_addr.port() + 1);

                let feedback_server = LVFeedbackServer::new(&feedback_addr.to_string());
                let bitrate_mtx = feedback_server.begin();

                let mut streaming_server = LVStreamingServer::new(
                    &addr,
                    &target_addr,
                    60,
                    0,
                    1920,
                    1080,
                    10000,
                    quit_rx,
                    bitrate_mtx,
                )?;

                streaming_server.begin();

                Ok(())
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
