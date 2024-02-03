use std::{
    net::UdpSocket,
    time::{Duration, Instant},
};

use crate::encoder::{nvidia::LVNvidiaEncoder, LVEncoder};
use crate::packager::LVPackager;
use crate::{
    capture::{linux::LVLinuxCapturer, LVCapturer},
    encoder::openh264_enc::LVOpenH264Encoder,
};
use log::{debug, error, info};
use screenshots::Screen;
use webrtc_util::{Marshal, MarshalSize};

const BITRATE: u32 = 250000;
const FRAMERATE: f32 = 60.0;
const BIND_ADDR: &'static str = "127.0.0.1:29878";
const ITERATIONS: u32 = 100;

pub fn bench() -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind(BIND_ADDR)?;
    let screen = *Screen::all()?.get(0).expect("Expected a screen");

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

    let encoder = LVNvidiaEncoder::new(width, height, BITRATE, FRAMERATE)?;
    let mut packager = LVPackager::new(Box::new(encoder), FRAMERATE as u32)?;

    // bad benchmark

    let sixty_fps = Duration::new(0, (1000000000. / 60.) as u32);

    let timer = Instant::now();

    // Statistics
    let mut capture_avg: u128 = 0;
    let mut process_avg: u128 = 0;
    let mut send_avg: u128 = 0;

    let mut rtp_pkt = vec![];

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
                // Don't always heap allocate
                rtp_pkt.resize(data.marshal_size(), 0);
                data.marshal_to(&mut rtp_pkt)?;
                let bytes = socket.send_to(&rtp_pkt, "127.0.0.1:22879")?;
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
