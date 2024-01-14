use std::thread;

use flume::{Receiver, Sender};
use lazy_static::lazy_static;

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
        thread::Builder::new()
            .name("statistics_thread".to_string())
            .spawn(|| {
                let mut stat = LVStatistics::new();

                while let Ok(data) = STAT_CH.1.recv() {
                    match data {
                        LVStatisticsMessage::RegisterData(s, t) => stat.register_data(s, t),
                        LVStatisticsMessage::UpdateData(s, p) => stat.update_data(s, p),
                        LVStatisticsMessage::Quit => {
                            let _ = stat.aggregate();
                        }
                    }
                }
            });
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
