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

#[cfg(test)]
mod test {
    use super::Device;

    #[test]
    fn test_new() {
        let name = "Test Device".to_owned();
        let manufacturer = Some("Test Manufacturer".into());
        let dev = Device::new(name.clone(), manufacturer.clone());

        assert_eq!(dev.name, name);
        assert_eq!(dev.manufacturer, manufacturer);
        assert_eq!(dev.identifiers.len(), 1);
        assert!(!dev.identifiers[0].contains(" "));
    }
}
