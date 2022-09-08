use serde::Serialize;
use std::rc::Rc;

use bollard::{
    container::Stats,
    service::{ContainerInspectResponse, ContainerStateStatusEnum, HealthStatusEnum},
};

use crate::{constants::UNKNOWN, mqtt::MqttClient};
#[derive(Serialize)]
pub struct Container {
    #[serde(skip)]
    mqtt: Rc<MqttClient>,
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
        mqtt: Rc<MqttClient>,
        stats: Stats,
        inspect: ContainerInspectResponse,
        image: Option<String>,
    ) -> Self {
        if inspect.name.is_some() && inspect.name.as_ref().unwrap() != &stats.name {
            log::warn!(
                "Creating container with stats name '{}' and inspect name '{}'",
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
            log::info!("Updating Container '{}'", self.name);

            self.state = Self::parse_state(&inspect);
            self.health = Self::parse_health(&inspect);
        }
    }

    pub async fn publish(&mut self) {
        match serde_json::to_string(&self) {
            Ok(json) => {
                if self.last_published.is_none() || self.last_published.as_ref().unwrap() != &json {
                    if let Ok(()) = self.mqtt.publish(&self.name[1..], &json, true, None).await {
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
        if let Err(e) = self.mqtt.publish(&self.name[1..], "", true, Some(1)).await {
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

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use bollard::{
        container::{
            BlkioStats, CPUStats, CPUUsage, MemoryStats, PidsStats, Stats, StorageStats,
            ThrottlingData,
        },
        service::{
            ContainerInspectResponse, ContainerState, ContainerStateStatusEnum, Health,
            HealthStatusEnum,
        },
    };

    use crate::{constants::UNKNOWN, mqtt::MqttClient, settings::Settings};

    use super::Container;

    #[test]
    fn test_new_container() {
        let name = "test_name";
        let image = "test-image";
        let mqtt = Rc::new(MqttClient::new(&Settings::new()).unwrap());

        let stats = get_stats(name);
        let inspect = ContainerInspectResponse {
            name: Some(name.into()),
            ..Default::default()
        };

        let container = Container::new(mqtt.clone(), stats, inspect, Some(image.into()));

        assert_eq!(container.name, name);
        assert_eq!(container.image, image);
        assert_eq!(container.last_published, None);
        assert!(Rc::ptr_eq(&container.mqtt, &mqtt));
    }

    #[test]
    fn test_new_container_no_image() {
        let name = "other_name";
        let mqtt = Rc::new(MqttClient::new(&Settings::new()).unwrap());

        let stats = get_stats(name);
        let inspect = ContainerInspectResponse {
            name: Some(name.into()),
            ..Default::default()
        };

        let container = Container::new(mqtt.clone(), stats, inspect, None);

        assert_eq!(container.name, name);
        assert_eq!(container.image, UNKNOWN.to_owned());
        assert_eq!(container.last_published, None);
        assert!(Rc::ptr_eq(&container.mqtt, &mqtt));
    }

    #[test]
    fn test_rename() {
        let name = "original_name";
        let mqtt = Rc::new(MqttClient::new(&Settings::new()).unwrap());

        let stats = get_stats(name);
        let inspect = ContainerInspectResponse {
            name: Some(name.into()),
            ..Default::default()
        };

        let mut container = Container::new(mqtt.clone(), stats, inspect, None);
        assert_eq!(container.name, name);

        let new_name = "renamed_name";
        container.rename(new_name.into());
        assert_eq!(container.name, new_name);
    }

    #[test]
    fn test_update() {
        let mqtt = Rc::new(MqttClient::new(&Settings::new()).unwrap());
        let stats = get_stats("");
        let mut inspect = ContainerInspectResponse {
            ..Default::default()
        };

        let mut container = Container::new(mqtt.clone(), stats.clone(), inspect.clone(), None);

        let status = ContainerStateStatusEnum::DEAD;
        let health = HealthStatusEnum::UNHEALTHY;
        inspect.state = Some(ContainerState {
            status: Some(status),
            health: Some(Health {
                status: Some(health),
                ..Default::default()
            }),
            ..Default::default()
        });
        container.update(stats, inspect);
        assert_eq!(container.state, status.as_ref());
        assert_eq!(container.health, health.as_ref());
    }

    #[test]
    fn test_parse_state() {
        let mut inspect = ContainerInspectResponse {
            ..Default::default()
        };
        assert_eq!(Container::parse_state(&inspect), UNKNOWN);

        // Test no status
        inspect.state = Some(ContainerState {
            ..Default::default()
        });
        assert_eq!(Container::parse_state(&inspect), UNKNOWN);

        // Test each status value
        let enum_values = [
            ContainerStateStatusEnum::EMPTY,
            ContainerStateStatusEnum::CREATED,
            ContainerStateStatusEnum::DEAD,
            ContainerStateStatusEnum::EXITED,
            ContainerStateStatusEnum::PAUSED,
            ContainerStateStatusEnum::REMOVING,
            ContainerStateStatusEnum::RESTARTING,
            ContainerStateStatusEnum::RUNNING,
        ];
        for val in enum_values {
            inspect.state = Some(ContainerState {
                status: Some(val),
                ..Default::default()
            });
            let expected = if val == ContainerStateStatusEnum::EMPTY {
                UNKNOWN
            } else {
                val.as_ref()
            };
            assert_eq!(Container::parse_state(&inspect), expected);
        }
    }

    #[test]
    fn test_parse_health() {
        let mut inspect = ContainerInspectResponse {
            ..Default::default()
        };
        assert_eq!(Container::parse_health(&inspect), UNKNOWN);

        // Test no health
        inspect.state = Some(ContainerState {
            ..Default::default()
        });
        assert_eq!(Container::parse_state(&inspect), UNKNOWN);

        // Test no health status
        inspect.state = Some(ContainerState {
            health: Some(Health {
                ..Default::default()
            }),
            ..Default::default()
        });
        assert_eq!(Container::parse_state(&inspect), UNKNOWN);

        // Test each health status
        let enum_values = [
            HealthStatusEnum::EMPTY,
            HealthStatusEnum::HEALTHY,
            HealthStatusEnum::NONE,
            HealthStatusEnum::STARTING,
            HealthStatusEnum::UNHEALTHY,
        ];
        for val in enum_values {
            inspect.state = Some(ContainerState {
                health: Some(Health {
                    status: Some(val),
                    ..Default::default()
                }),
                ..Default::default()
            });
            let expected = if val == HealthStatusEnum::EMPTY {
                UNKNOWN
            } else {
                val.as_ref()
            };
            assert_eq!(Container::parse_health(&inspect), expected);
        }
    }

    fn get_stats(name: &str) -> Stats {
        let cpu = CPUStats {
            cpu_usage: CPUUsage {
                percpu_usage: None,
                total_usage: 1,
                usage_in_kernelmode: 1,
                usage_in_usermode: 1,
            },
            online_cpus: None,
            system_cpu_usage: None,
            throttling_data: ThrottlingData {
                periods: 1,
                throttled_periods: 1,
                throttled_time: 1,
            },
        };
        Stats {
            id: "".into(),
            read: "".into(),
            preread: "".into(),
            num_procs: 1,
            pids_stats: PidsStats {
                current: None,
                limit: None,
            },
            network: None,
            blkio_stats: BlkioStats {
                io_merged_recursive: None,
                io_queue_recursive: None,
                io_service_bytes_recursive: None,
                io_service_time_recursive: None,
                io_serviced_recursive: None,
                io_time_recursive: None,
                io_wait_time_recursive: None,
                sectors_recursive: None,
            },
            cpu_stats: cpu.clone(),
            memory_stats: MemoryStats {
                commit: None,
                commit_peak: None,
                commitbytes: None,
                commitpeakbytes: None,
                failcnt: None,
                limit: None,
                max_usage: None,
                privateworkingset: None,
                stats: None,
                usage: None,
            },
            name: name.into(),
            networks: None,
            precpu_stats: cpu,
            storage_stats: StorageStats {
                read_count_normalized: None,
                read_size_bytes: None,
                write_count_normalized: None,
                write_size_bytes: None,
            },
        }
    }
}
