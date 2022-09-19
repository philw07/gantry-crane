pub const APP_NAME: &str = env!("CARGO_PKG_NAME");
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const PRECISION: u32 = 2;

pub const BASE_TOPIC: &str = "gantry-crane";
pub const AVAILABILITY_TOPIC: &str = "availability";
pub const SET_STATE_TOPIC: &str = "set";
pub const STATE_ONLINE: &str = "online";
pub const STATE_OFFLINE: &str = "offline";
pub const UNKNOWN: &str = "unknown";

pub const DOCKER_EVENT_ACTION_CREATE: &str = "create";
pub const DOCKER_EVENT_ACTION_DESTROY: &str = "destroy";
pub const DOCKER_EVENT_ACTION_RENAME: &str = "rename";
pub const DOCKER_EVENT_ACTION_START: &str = "start";
pub const DOCKER_EVENT_ACTION_STOP: &str = "stop";
pub const DOCKER_EVENT_ACTION_RESTART: &str = "restart";
pub const DOCKER_EVENT_ACTION_PAUSE: &str = "pause";
pub const DOCKER_EVENT_ACTION_UNPAUSE: &str = "unpause";
