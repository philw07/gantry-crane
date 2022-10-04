use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Device {
    pub name: String,
    pub identifiers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sw_version: Option<String>,
}

impl Device {
    pub fn new(name: String, manufacturer: Option<String>) -> Self {
        let id = format!("gc_{}", name.replace(' ', "_"));
        Self {
            name,
            identifiers: vec![id],
            manufacturer,
            model: None,
            sw_version: None,
        }
    }
}
