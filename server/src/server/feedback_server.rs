use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{debug, error, info};
use net::feedback_packet::{LVAck, LVFeedbackPacket, ACK_TYPE, FEEDBACK_TYPE};
use statistics::collector::LVStatisticsCollector;
use statistics::statistics::{LVDataPoint, LVDataType};

pub struct LVFeedbackServer {
    bind_addr: String,
}

impl LVFeedbackServer {
    pub fn new(bind_addr: &str) -> Self {
        Self {
            bind_addr: bind_addr.to_owned(),
        }
    }

    fn handle_feedback(mut stream: TcpStream, bitrate_mtx: Arc<Mutex<u32>>) {
        // The +1 should be for the extra byte that tells us what type of packet this is.
        let mut msg_buffer =
            vec![0; std::cmp::max(LVFeedbackPacket::no_bytes(), LVAck::no_bytes()) + 1];
        let mut bitrate = 80000;
        let mut oo_blocks = 0;
        let mut decoder_failures = 0;

        LVStatisticsCollector::register_data("server_bitrate_oo_blocks", LVDataType::XYData);
        LVStatisticsCollector::register_data("server_rtt_time", LVDataType::XYData);
        LVStatisticsCollector::register_data("server_rtt_bitrate", LVDataType::XYData);
        LVStatisticsCollector::register_data(
            "server_bitrate_ecc_decoder_failures",
            LVDataType::XYData,
        );

        loop {
            match stream.read(&mut msg_buffer[..]) {
                Ok(data_read) => {
                    let feedback_type = msg_buffer[0];
                    info!("feedback type is {}", feedback_type);
                    match feedback_type {
                        ACK_TYPE => {
                            info!(
                                "ack packet to be decoded is {:?}",
                                &msg_buffer[1..(LVAck::no_bytes() + 1)]
                            );
                            let ack: LVAck =
                                bincode::deserialize::<LVAck>(&msg_buffer[1..]).unwrap();

                            // 1. Calculate RTT
                            let current_time = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis();
                            let rtt = current_time - ack.send_ts;
                            info!("rtt was {}", rtt);

                            // NOTE: We hope that the u128 when they are subtracted will fit within an f32.
                            LVStatisticsCollector::update_data(
                                "server_rtt_time",
                                LVDataPoint::XYValue((ack.rtp_seqno as f32, rtt as f32)),
                            );
                            LVStatisticsCollector::update_data(
                                "server_rtt_bitrate",
                                LVDataPoint::XYValue((bitrate as f32, rtt as f32)),
                            );
                            info!("ack packet is {:?}", ack);
                        }
                        FEEDBACK_TYPE => {
                            debug!(
                                "Feedback packet to be decoded is {:?}",
                                &msg_buffer[1..LVFeedbackPacket::no_bytes() + 1]
                            );
                            match bincode::deserialize::<LVFeedbackPacket>(
                                &msg_buffer[1..LVFeedbackPacket::no_bytes() + 1],
                            ) {
                                Ok(feedback_packet) => {
                                    debug!("Feedback packet is {:?}", feedback_packet);

                                    // Algorithm: we have the following information:
                                    // - time quantum
                                    // - total blocks
                                    // - out of order blocks
                                    // - total_packets (not using this yet)
                                    // - lost_packets (not using this yet)
                                    // - total RS decoder failures

                                    // congestion = [(out of order blocks)/(total blocks)]

                                    // bitrate =
                                    //   (bitrate + 200) if congestion > 0.2
                                    //   (bitrate) if 0.15 < congestion > 0.2 -- stable
                                    //   (bitrate * 0.5) if (congestion < 0.15) or (decoder_failures > 0)

                                    let congestion = feedback_packet.out_of_order_blocks as f32
                                        / feedback_packet.total_blocks as f32;

                                    debug!("congestion is {}", congestion);

                                    {
                                        let mut bitrate_mtx_set = bitrate_mtx
                                            .lock()
                                            .expect("Failed to lock bitrate mutex");

                                        *bitrate_mtx_set = {
                                            if congestion > 0.001
                                                || feedback_packet.ecc_decoder_failures > 0
                                            {
                                                // this multiplication is not just integer division
                                                // in case we want to change the multiplication constant
                                                // later
                                                (bitrate as f32 * 0.6) as u32
                                            } else if congestion < 0.2 && congestion > 0.15 {
                                                bitrate
                                            } else {
                                                bitrate + 400000
                                                // (bitrate as f32 - 1000) as u32
                                            }
                                        };

                                        // bitrate changed
                                        oo_blocks += feedback_packet.out_of_order_blocks;
                                        decoder_failures += feedback_packet.ecc_decoder_failures;
                                        if bitrate != *bitrate_mtx_set {
                                            LVStatisticsCollector::update_data(
                                                "server_bitrate_oo_blocks",
                                                LVDataPoint::XYValue((
                                                    bitrate as f32,
                                                    oo_blocks as f32,
                                                )),
                                            );
                                            LVStatisticsCollector::update_data(
                                                "server_bitrate_ecc_decoder_failures",
                                                LVDataPoint::XYValue((
                                                    bitrate as f32,
                                                    decoder_failures as f32,
                                                )),
                                            );
                                            oo_blocks = 0;
                                            decoder_failures = 0;
                                        }

                                        // So we don't have to lock the mutex unnecessarily
                                        bitrate = *bitrate_mtx_set;

                                        debug!("setting bitrate to {}", bitrate);
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to decode feedback packet with error {:?}", e)
                                }
                            }
                        }
                        _ => {
                            error!("unknown feedback packet type!")
                        }
                    }
                }
                Err(e) => error!("Could not read bytes from client {:?}", e),
            }
        }
    }

    pub fn start_receive_loop(
        bind_addr: &str,
        bitrate_shared: Arc<Mutex<u32>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("connecting to feedback server at {}", bind_addr);
        let tcp_stream = TcpStream::connect(bind_addr)?;

        info!("connected to feedback server at {}", bind_addr);

        Self::handle_feedback(tcp_stream, bitrate_shared.clone());

        Ok(())
    }

    pub fn begin(&self) -> Arc<Mutex<u32>> {
        info!("Starting feedback server");
        let bitrate_shared = Arc::new(Mutex::new(80000));
        let bitrate_shared_clone = bitrate_shared.clone();
        let bind_addr_clone = self.bind_addr.clone();
        thread::spawn(move || {
            Self::start_receive_loop(&bind_addr_clone, bitrate_shared_clone)
                .expect("Failed to start feedback server");
        });
        bitrate_shared
    }
}
