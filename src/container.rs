use serde::Serialize;
use std::rc::Rc;
use tokio::sync::RwLock;

use bollard::{
    container::Stats,
    service::{ContainerInspectResponse, ContainerStateStatusEnum, HealthStatusEnum},
};

use crate::{constants::UNKNOWN, mqtt::MqttClient};
#[derive(Serialize)]
pub struct Container {
    #[serde(skip)]
    mqtt: Rc<RwLock<MqttClient>>,
    #[serde(skip)]
    last_published: Option<String>,

    #[serde(serialize_with = "serialize_name")]
    name: String,
    image: String,

    state: String,
    health: String,
}

impl Container {
    pub fn new(
        mqtt: Rc<RwLock<MqttClient>>,
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
            mqtt,
            last_published: None,

            name: stats.name.clone(),
            image: image.unwrap_or_else(|| UNKNOWN.into()),

            state: Self::parse_state(&inspect),
            health: Self::parse_health(&inspect),
        };
        container.update(stats, inspect);
        container
    }

    pub fn get_name(&self) -> &str {
        &self.name
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

            self.state = Self::parse_state(&inspect);
            self.health = Self::parse_health(&inspect);
        }
    }

    pub async fn publish(&mut self) {
        match serde_json::to_string(&self) {
            Ok(json) => {
                if self.last_published.is_none() || self.last_published.as_ref().unwrap() != &json {
                    if let Ok(()) = self
                        .mqtt
                        .read()
                        .await
                        .publish(&self.name[1..], &json, true, None)
                        .await
                    {
                        self.last_published = Some(json);
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to serialize container '{}': {}", self.name, e)
            }
        };
    }

    pub async fn unpublish(&self) {
        if let Err(e) = self
            .mqtt
            .read()
            .await
            .publish(&self.name[1..], "", true, Some(1))
            .await
        {
            log::error!("Failed to unpublish container '{}': {}", self.name, e);
        }
    }

    fn parse_state(inspect: &ContainerInspectResponse) -> String {
        if let Some(ref state) = inspect.state {
            match state.status {
                Some(ContainerStateStatusEnum::EMPTY) => UNKNOWN.into(),
                Some(status) => status.as_ref().into(),
                None => UNKNOWN.into(),
            }
        } else {
            UNKNOWN.into()
        }
    }

    fn parse_health(inspect: &ContainerInspectResponse) -> String {
        if let Some(ref state) = inspect.state {
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
        }
    }
}

fn serialize_name<S>(name: &str, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_str(&name[1..])
}
