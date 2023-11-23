use std::{
    net::UdpSocket,
    time::{Duration, Instant},
};

use capture::{linux::LVLinuxCapturer, LVCapturer};
use encoder::LVEncoder;
use flexi_logger::Logger;
use log::{debug, error, info};
use packager::LVPackager;
use screenshots::Screen;

mod capture;
mod encoder;
mod packager;
mod sender;

const BITRATE: u32 = 100000;
const FRAMERATE: f32 = 60.0;
const BIND_ADDR: &'static str = "127.0.0.1:29878";
const ITERATIONS: u32 = 10;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Logger::try_with_str("debug")?.start()?;

    let socket = UdpSocket::bind(BIND_ADDR)?;
    let screen = *Screen::all()?.get(1).expect("Expected a screen");

    // Screen size from screen is unreliable, so we'll get it from the capture instead.

    let mut capturer = LVLinuxCapturer::new(screen)?;
    // Capture a frame to figure out the frame size
    let (width, height) = match capturer.capture() {
        Ok(frame) => (frame.width(), frame.height()),
        Err(e) => {
            error!("captured frame was None! {:#?}", e);
            panic!();
        }
    };

    info!(
        "initialsing LVEncoder with screen size: {}x{}",
        width, height
    );

    let encoder = LVEncoder::new(width, height, BITRATE, FRAMERATE)?;
    let mut packager = LVPackager::new(encoder)?;

    // bad benchmark

    let sixty_fps = Duration::new(0, (1000000000. / 60.) as u32);

    let timer = Instant::now();

    // Statistics
    let mut capture_avg: u128 = 0;
    let mut process_avg: u128 = 0;
    let mut send_avg: u128 = 0;

    for i in 0..ITERATIONS {
        let before = Instant::now();
        info!("starting capture -> package -> send benchmark");
        if let Ok(frame) = capturer.capture() {
            let elapsed = before.elapsed();
            capture_avg += elapsed.as_millis();
            info!("ITERATION {} capture elapsed time: {:.4?}", i, elapsed);

            // Encode/package frame
            let before = Instant::now();
            let _ = packager.process_frame(frame, timer.elapsed().as_millis() as u64)?;
            let elapsed = before.elapsed();
            process_avg += elapsed.as_millis();
            info!(
                "ITERATION {} process_frame elapsed time: {:.4?}",
                i, elapsed
            );

            // Send frame over network
            let before = Instant::now();
            while let Some(data) = packager.pop_rtp() {
                let bytes = socket.send_to(&data, "127.0.0.1:22879")?;
                debug!("sent {} bytes to addr", bytes);
            }
            let elapsed = before.elapsed();
            send_avg += elapsed.as_millis();
            info!("ITERATION {} socket send_to: {:.4?}", i, elapsed);
        } else {
            error!("captured frame was None!");
            panic!()
        }
        spin_sleep::sleep(sixty_fps);
    }

    let capture_avg = capture_avg as f64 / ITERATIONS as f64;
    let process_avg = process_avg as f64 / ITERATIONS as f64;
    let send_avg = send_avg as f64 / ITERATIONS as f64;

    info!(
        "\nSTATISTICS:\n\tITERATIONS: {}\n\tcapture_avg: {}\n\tprocess_avg: {}\n\tsend_avg: {}\n\tTOTAL_AVG: {}",
        ITERATIONS,
        capture_avg,
        process_avg,
        send_avg,
        capture_avg + process_avg + send_avg
    );

    Ok(())
}
