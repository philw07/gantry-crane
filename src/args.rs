use argh::FromArgs;

#[derive(Debug, FromArgs)]
/// A Docker to MQTT bridge
pub struct GantryCraneArgs {
    /// remove all retained MQTT topics
    #[argh(switch)]
    pub clean: bool,

    /// provide the path to the config file
    #[argh(option)]
    pub config: Option<String>,

    /// display the version of the app
    #[argh(switch, short = 'v')]
    pub version: bool,
}

impl GantryCraneArgs {
    pub fn from_env() -> Self {
        argh::from_env()
    }
}
