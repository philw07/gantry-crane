use bollard::{
    container::Stats,
    service::{ContainerInspectResponse, ContainerStateStatusEnum, HealthStatusEnum},
};

const UNKNOWN: &str = "unknown";

#[derive(Debug, Default)]
pub struct Container {
    name: String,
    image: String,

    state: String,
    health: String,
}

impl Container {
    pub fn from_stats_and_inspect(
        stats: Stats,
        inspect: ContainerInspectResponse,
        image: Option<String>,
    ) -> Self {
        if inspect.name.is_some() && inspect.name.as_ref().unwrap() != &stats.name {
            log::warn!(
                "Creating container with stats '{}' and inspect '{}'",
                stats.name,
                inspect.name.as_ref().unwrap()
            );
        }

        let mut container = Container {
            name: stats.name.clone(),
            image: image.unwrap_or_else(|| UNKNOWN.into()),
            ..Default::default()
        };
        container.update(stats, inspect);
        container
    }

    pub fn rename(&mut self, new_name: String) {
        self.name = new_name;
    }

    pub fn update(&mut self, stats: Stats, inspect: ContainerInspectResponse) {
        if self.name != stats.name
            || inspect.name.is_some() && inspect.name.as_ref().unwrap() != &stats.name
        {
            log::warn!(
                "Container '{}' received stats update for '{}' and inspect for '{}'",
                self.name,
                stats.name,
                inspect.name.as_deref().unwrap_or("<None>")
            )
        } else {
            log::debug!("Updating Container '{}'", self.name);

            self.state = if let Some(ref state) = inspect.state {
                match state.status {
                    Some(ContainerStateStatusEnum::EMPTY) => UNKNOWN.into(),
                    Some(status) => status.as_ref().into(),
                    None => UNKNOWN.into(),
                }
            } else {
                UNKNOWN.into()
            };

            self.health = if let Some(ref state) = inspect.state {
                match state.health {
                    Some(ref health) => match health.status {
                        Some(HealthStatusEnum::EMPTY) => UNKNOWN.into(),
                        Some(status) => status.as_ref().into(),
                        None => UNKNOWN.into(),
                    },
                    None => UNKNOWN.into(),
                }
            } else {
                UNKNOWN.into()
            };
        }
    }
}
