pub const APP_NAME: &str = env!("CARGO_PKG_NAME");
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CONFIG_FILE: &str = "gantry-crane.toml";

pub const PRECISION: u32 = 2;

pub const AVAILABILITY_TOPIC: &str = "availability";
pub const SET_STATE_TOPIC: &str = "set";
pub const STATE_ONLINE: &str = "online";
pub const STATE_OFFLINE: &str = "offline";
pub const UNKNOWN: &str = "unknown";

pub const DOCKER_LABEL_FILTER: &str = "gantry-crane.enable";

pub const DOCKER_EVENT_ACTION_CREATE: &str = "create";
pub const DOCKER_EVENT_ACTION_DESTROY: &str = "destroy";
pub const DOCKER_EVENT_ACTION_RENAME: &str = "rename";
pub const DOCKER_EVENT_ACTION_START: &str = "start";
pub const DOCKER_EVENT_ACTION_STOP: &str = "stop";
pub const DOCKER_EVENT_ACTION_RESTART: &str = "restart";
pub const DOCKER_EVENT_ACTION_PAUSE: &str = "pause";
pub const DOCKER_EVENT_ACTION_UNPAUSE: &str = "unpause";
pub const DOCKER_NETWORK_MODE_HOST: &str = "host";

pub const CONTAINER_REQUEST_START: &str = DOCKER_EVENT_ACTION_START;
pub const CONTAINER_REQUEST_STOP: &str = DOCKER_EVENT_ACTION_STOP;
pub const CONTAINER_REQUEST_RESTART: &str = DOCKER_EVENT_ACTION_RESTART;
pub const CONTAINER_REQUEST_PAUSE: &str = DOCKER_EVENT_ACTION_PAUSE;
pub const CONTAINER_REQUEST_UNPAUSE: &str = DOCKER_EVENT_ACTION_UNPAUSE;
pub const CONTAINER_REQUEST_RECREATE: &str = "recreate";
pub const CONTAINER_REQUEST_PULL_RECREATE: &str = "pull_recreate";

// These numbers should be more than sufficient for the usual home setup
pub const BUFFER_SIZE_EVENT_CHANNEL: usize = 1024;
pub const BUFFER_SIZE_POLL_CHANNEL: usize = 8;
pub const BUFFER_SIZE_MQTT_RECV: usize = 384;
pub const BUFFER_SIZE_MQTT_SEND: i32 = 384;
