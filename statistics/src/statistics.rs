use std::{collections::HashMap, fs::File, io::Write, path::Path, time::Duration};

use chrono::Utc;
use log::{debug, warn};

const STATOUT: &'static str = "statout";

pub enum LVDataType {
    TimeSeries,
    Aggregate,
}

#[derive(Debug)]
pub enum LVDataPoint {
    TimeElapsed(Duration),
    FloatValue(f32),
    Increment,
}

#[derive(Debug)]
enum LVStoredData {
    TimeSeries(Vec<LVDataPoint>),

    // Some kind of counter, eg. number of dropped packets/frames
    Aggregate(usize),
}

#[derive(Debug, serde::Serialize)]
struct LVTimeSeriesDuration {
    index: usize,
    time_elapsed: Duration,
}

#[derive(Debug, serde::Serialize)]
struct LVTimeSeriesFloat {
    index: usize,
    value: f32,
}

pub struct LVStatistics {
    data: HashMap<String, LVStoredData>,
}

impl LVStatistics {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn register_data(&mut self, data_name: String, data_type: LVDataType) {
        match data_type {
            LVDataType::TimeSeries => {
                self.data
                    .insert(data_name, LVStoredData::TimeSeries(vec![]));
            }
            LVDataType::Aggregate => {
                self.data.insert(data_name, LVStoredData::Aggregate(0));
            }
        }
    }

    // This is a horrible API, but I don't really feel like making it better right now.
    pub fn update_data(&mut self, data_name: String, data_point: LVDataPoint) {
        if let Some(mut data_value) = self.data.get_mut(&data_name) {
            match &mut data_value {
                LVStoredData::TimeSeries(series) => {
                    if let LVDataPoint::Increment = data_point {
                        warn!(
                            "Stored data is type TimeSeries but data_point given was {:?}",
                            data_point
                        )
                    } else {
                        series.push(data_point);
                    }
                }
                LVStoredData::Aggregate(current_val) => {
                    if let LVDataPoint::Increment = data_point {
                        *current_val += 1;
                    } else {
                        warn!(
                            "Stored data is type aggregate but data_point given was {:?}",
                            data_point
                        )
                    }
                }
            }
        }
        else {
            warn!("could not find data {:?} in HashMap", data_name)
        }
    }

    pub fn aggregate(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Output data to STATOUT
        let out_path = Path::new(STATOUT).join(Utc::now().to_string());
        std::fs::create_dir_all(&out_path)?;

        for (data_name, value) in &self.data {
            let out_file = out_path.join(data_name);
            match value {
                LVStoredData::TimeSeries(ts) => {
                    // durations are converted to ms
                    let mut f = File::create(out_file)?;
                    // convert data to csv
                    if ts.len() > 0 {
                        match ts[0] {
                            LVDataPoint::TimeElapsed(_) => {
                                write!(f, "index,time_elapsed\n")?;
                                for (i, x) in ts.iter().enumerate() {
                                    write!(f, "{},{}\n", i, match x {
                                                    LVDataPoint::TimeElapsed(x) => x.as_nanos(),
                                                    _ => panic!("Mixing data point types in a single time series is not supported"),
                                    })?;
                                }
                            }
                            LVDataPoint::FloatValue(_) => {
                                write!(f, "index,value\n")?;
                                for (i, x) in ts.iter().enumerate() {
                                    write!(f, "{},{}", i, match x {
                                            LVDataPoint::FloatValue(x) => *x,
                                            _ => panic!("Mixing data point types in a single time series is not supported"),
                                    })?;
                                    f.write(b"\n")?;
                                }
                            }
                            _ => panic!("Cannot have Increment in a time series"),
                        }
                    }
                }
                LVStoredData::Aggregate(aggregated_value) => {
                    std::fs::write(out_file, aggregated_value.to_string())?;
                }
            }
        }
        Ok(())
    }
}
