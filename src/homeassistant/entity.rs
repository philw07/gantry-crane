use serde::Serialize;

pub trait Entity<T: Serialize = Self>: Serialize {
    fn topic(&self, base_topic: &str, node_id: &str) -> String;
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum DeviceClass {
    Restart,
    Update,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum EntityCategory {
    Config,
    Diagnostic,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum StateClass {
    Measurement,
    Total,
    TotalIncreasing,
}
