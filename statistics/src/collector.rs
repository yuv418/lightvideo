use std::thread;

use flume::{Receiver, Sender};
use lazy_static::lazy_static;
use log::info;

use crate::statistics::{LVDataPoint, LVDataType, LVStatistics};

pub enum LVStatisticsMessage {
    RegisterData(String, LVDataType),
    UpdateData(String, LVDataPoint),
    Quit,
}

pub struct LVStatisticsCollector {}

lazy_static! {
    static ref STAT_CH: (Sender<LVStatisticsMessage>, Receiver<LVStatisticsMessage>) =
        flume::unbounded();
}

impl LVStatisticsCollector {
    pub fn start() {
        let (quit_tx, quit_rx) = flume::bounded::<bool>(1);

        let t = thread::Builder::new()
            .name("statistics_thread".to_string())
            .spawn(move || {
                let mut stat = LVStatistics::new();

                while let Ok(data) = STAT_CH.1.recv() {
                    match data {
                        LVStatisticsMessage::RegisterData(s, t) => stat.register_data(s, t),
                        LVStatisticsMessage::UpdateData(s, p) => stat.update_data(s, p),
                        LVStatisticsMessage::Quit => {
                            let _ = stat.aggregate();
                            quit_tx.send(true).expect("failed to send quit signal");

                            return;
                        }
                    }
                }
            })
            .expect("Failed to start statistics thread");

        ctrlc::set_handler(move || {
            info!("Writing statistics after ctrl-c");
            let _ = STAT_CH.0.send(LVStatisticsMessage::Quit);
            if let Ok(true) = quit_rx.recv() {
                info!("aggregation finished.");
                std::process::exit(0)
            }
        })
        .expect("Failed to set ctrlc handler");
    }

    pub fn register_data(key: &str, data_type: LVDataType) {
        let _ = STAT_CH.0.send(LVStatisticsMessage::RegisterData(
            key.to_string(),
            data_type,
        ));
    }

    pub fn update_data(key: &str, data_pt: LVDataPoint) {
        let _ = STAT_CH
            .0
            .send(LVStatisticsMessage::UpdateData(key.to_string(), data_pt));
    }

    pub fn quit() {
        let _ = STAT_CH.0.send(LVStatisticsMessage::Quit);
    }
}
