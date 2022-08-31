pub struct Settings {
    pub poll_interval: u32,
}

impl Settings {
    pub fn new() -> Self {
        Settings { poll_interval: 60 }
    }
}
