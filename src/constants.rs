pub const APP_NAME: &str = env!("CARGO_PKG_NAME");
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const BASE_TOPIC: &str = "gantry-crane";
pub const STATE_TOPIC: &str = "bridge/state";
pub const STATE_ONLINE: &str = "online";
pub const STATE_OFFLINE: &str = "offline";
pub const UNKNOWN: &str = "unknown";

pub const DOCKER_EVENT_ACTION_CREATE: &str = "create";
pub const DOCKER_EVENT_ACTION_DESTROY: &str = "destroy";
pub const DOCKER_EVENT_ACTION_RENAME: &str = "rename";
