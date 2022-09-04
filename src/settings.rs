pub struct Settings {
    // Docker related settings
    pub poll_interval: u32,

    // MQTT related settings
    pub mqtt_host: String,
    pub mqtt_port: u16,
}

impl Settings {
    pub fn new() -> Self {
        Settings {
            poll_interval: 60,
            mqtt_host: "localhost".into(),
            mqtt_port: 1883,
        }
    }
}
