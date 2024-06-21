use std::net::{Ipv4Addr, SocketAddr};

use flexi_logger::Logger;
use input::x11::LVX11InputEmulator;
use log::debug;
use server::{
    feedback_server::LVFeedbackServer, input_server::LVInputServer,
    streaming_server::LVStreamingServer,
};
use statistics::collector::LVStatisticsCollector;

mod benchmark;
mod capture;
mod encoder;
mod input;
mod packager;
mod server;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Logger::try_with_str(
        "trace,server::input=info,server::server::feedback_server=debug,statistics=info,server::server::streaming_server=info, server::server::input_server=info, server::packager=info, server::capture=info, server::encoder=info, net=info",
    )?
    .start()?;
    let quit_rx = LVStatisticsCollector::start();

    match std::env::args().nth(1).as_deref() {
        Some("bench") => benchmark::bench(),
        Some("server") => match std::env::args().nth(2) {
            Some(addr) => {
                let target_addr = std::env::args().nth(3).unwrap();

                let mut feedback_addr: SocketAddr = target_addr.parse()?;
                feedback_addr.set_port(feedback_addr.port() + 2);
                let feedback_server = LVFeedbackServer::new(&feedback_addr.to_string());

                let mut input_addr: SocketAddr = addr.parse()?;
                let mut input_target_addr: SocketAddr = target_addr.parse()?;
                input_target_addr.set_port(input_target_addr.port() + 3);
                input_addr.set_port(input_addr.port() + 3);

                let input_server = LVInputServer::new(&input_addr.to_string());
                let input_emulator = Box::new(LVX11InputEmulator::new()?);

                let bitrate_mtx = feedback_server.begin();

                let mut streaming_server = LVStreamingServer::new(
                    &addr,
                    &target_addr,
                    60,
                    0,
                    1920,
                    1080,
                    900000,
                    quit_rx,
                    bitrate_mtx,
                )?;

                input_server.start_receive_loop(input_target_addr, input_emulator);
                streaming_server.begin()?;

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
