use serde::Serialize;
use std::rc::Rc;

use bollard::{
    container::{MemoryStatsStats, Stats},
    service::{ContainerInspectResponse, ContainerStateStatusEnum, HealthStatusEnum},
};

use crate::{
    constants::{PRECISION, UNKNOWN},
    mqtt::MqttClient,
    util::round,
};

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

    cpu_percentage: f64,
    mem_percentage: f64,
    mem: f64,
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

            state: UNKNOWN.into(),
            health: UNKNOWN.into(),

            cpu_percentage: 0.0,
            mem_percentage: 0.0,
            mem: 0.0,
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

            self.parse_state(&inspect);
            self.parse_health(&inspect);
            self.parse_cpu(&stats);
            self.parse_mem(&stats);
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

    fn parse_state(&mut self, inspect: &ContainerInspectResponse) {
        self.state = if let Some(ref state) = inspect.state {
            match state.status {
                Some(ContainerStateStatusEnum::EMPTY) => UNKNOWN.into(),
                Some(status) => status.as_ref().into(),
                None => UNKNOWN.into(),
            }
        } else {
            UNKNOWN.into()
        }
    }

    fn parse_health(&mut self, inspect: &ContainerInspectResponse) {
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
        }
    }

    fn parse_cpu(&mut self, stats: &Stats) {
        let mut cpu = 0.0;
        if let Some(sys_usage) = stats.cpu_stats.system_cpu_usage {
            if let Some(percpu_usage) = stats.cpu_stats.cpu_usage.percpu_usage.as_ref() {
                let cpu_delta = stats.cpu_stats.cpu_usage.total_usage as f64
                    - stats.precpu_stats.cpu_usage.total_usage as f64;
                let system_delta =
                    sys_usage as f64 - stats.precpu_stats.system_cpu_usage.unwrap_or(0) as f64;

                if cpu_delta > 0.0 && system_delta > 0.0 {
                    cpu = (cpu_delta / system_delta) * 100.0 * percpu_usage.len() as f64;
                }
            }
        }

        self.cpu_percentage = round(cpu, PRECISION)
    }

    fn parse_mem(&mut self, stats: &Stats) {
        let mut mem = 0.0;
        let mut percentage = 0.0;
        if let Some(usage) = stats.memory_stats.usage {
            mem = usage as f64;
            // Subtract cache if available (same as docker CLI)
            if let Some(MemoryStatsStats::V1(stats_v1)) = stats.memory_stats.stats {
                mem -= stats_v1.total_inactive_file as f64;
            }

            // Calculate percentage
            if let Some(limit) = stats.memory_stats.limit {
                percentage = mem / limit as f64 * 100.0;
            }

            // Convert to MiB
            mem /= 1_048_576.0;
        }

        self.mem = round(mem, PRECISION);
        self.mem_percentage = round(percentage, PRECISION);
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
            BlkioStats, CPUStats, CPUUsage, MemoryStats, MemoryStatsStats, MemoryStatsStatsV1,
            PidsStats, Stats, StorageStats, ThrottlingData,
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
        let mqtt = Rc::new(MqttClient::new(&Settings::new().unwrap()).unwrap());

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
        let mqtt = Rc::new(MqttClient::new(&Settings::new().unwrap()).unwrap());

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
        let mqtt = Rc::new(MqttClient::new(&Settings::new().unwrap()).unwrap());

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
        let mqtt = Rc::new(MqttClient::new(&Settings::new().unwrap()).unwrap());
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
        let mqtt = Rc::new(MqttClient::new(&Settings::new().unwrap()).unwrap());
        let stats = get_stats("");
        let mut inspect = ContainerInspectResponse {
            ..Default::default()
        };
        let mut container = Container::new(mqtt.clone(), stats.clone(), inspect.clone(), None);

        assert_eq!(container.state, UNKNOWN);

        // Test no status
        inspect.state = Some(ContainerState {
            ..Default::default()
        });
        container.parse_state(&inspect);
        assert_eq!(container.state, UNKNOWN);

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
            container.parse_state(&inspect);
            assert_eq!(container.state, expected);
        }
    }

    #[test]
    fn test_parse_health() {
        let mqtt = Rc::new(MqttClient::new(&Settings::new().unwrap()).unwrap());
        let stats = get_stats("");
        let mut inspect = ContainerInspectResponse {
            ..Default::default()
        };
        let mut container = Container::new(mqtt.clone(), stats.clone(), inspect.clone(), None);

        assert_eq!(container.health, UNKNOWN);

        // Test no health
        inspect.state = Some(ContainerState {
            ..Default::default()
        });
        container.parse_health(&inspect);
        assert_eq!(container.health, UNKNOWN);

        // Test no health status
        inspect.state = Some(ContainerState {
            health: Some(Health {
                ..Default::default()
            }),
            ..Default::default()
        });
        container.parse_health(&inspect);
        assert_eq!(container.health, UNKNOWN);

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
            container.parse_health(&inspect);
            assert_eq!(container.health, expected);
        }
    }

    #[test]
    fn test_parse_cpu() {
        let mqtt = Rc::new(MqttClient::new(&Settings::new().unwrap()).unwrap());
        let mut stats = get_stats("");
        let inspect = ContainerInspectResponse {
            ..Default::default()
        };
        let mut container = Container::new(mqtt.clone(), stats.clone(), inspect.clone(), None);

        stats.cpu_stats.cpu_usage.total_usage = 20;
        stats.cpu_stats.system_cpu_usage = Some(200);
        stats.precpu_stats.cpu_usage.total_usage = 10;
        stats.precpu_stats.system_cpu_usage = Some(100);
        container.parse_cpu(&stats);
        assert_eq!(container.cpu_percentage, 0.0);

        stats.cpu_stats.cpu_usage.percpu_usage = Some(vec![0; 2]);
        container.parse_cpu(&stats);
        assert_eq!(container.cpu_percentage, 20.0);

        stats.cpu_stats.cpu_usage.percpu_usage = Some(vec![0; 4]);
        container.parse_cpu(&stats);
        assert_eq!(container.cpu_percentage, 40.0);

        stats.cpu_stats.cpu_usage.total_usage = 11;
        container.parse_cpu(&stats);
        assert_eq!(container.cpu_percentage, 4.0);

        stats.cpu_stats.system_cpu_usage = Some(150);
        container.parse_cpu(&stats);
        assert_eq!(container.cpu_percentage, 8.0);

        stats.cpu_stats.system_cpu_usage = None;
        container.parse_cpu(&stats);
        assert_eq!(container.cpu_percentage, 0.0);
    }

    #[test]
    fn test_parse_mem() {
        let mqtt = Rc::new(MqttClient::new(&Settings::new().unwrap()).unwrap());
        let mut stats = get_stats("");
        let inspect = ContainerInspectResponse {
            ..Default::default()
        };
        let mut container = Container::new(mqtt.clone(), stats.clone(), inspect.clone(), None);

        assert_eq!(container.mem, 0.0);
        assert_eq!(container.mem_percentage, 0.0);

        stats.memory_stats.usage = Some(100 * 1_048_576);
        container.parse_mem(&stats);
        assert_eq!(container.mem, 100.0);
        assert_eq!(container.mem_percentage, 0.0);

        stats.memory_stats.limit = Some(1000 * 1_048_576);
        container.parse_mem(&stats);
        assert_eq!(container.mem, 100.0);
        assert_eq!(container.mem_percentage, 10.0);

        stats.memory_stats.stats = Some(MemoryStatsStats::V1(get_memory_stats_v1(10 * 1_048_576)));
        container.parse_mem(&stats);
        assert_eq!(container.mem, 90.0);
        assert_eq!(container.mem_percentage, 9.0);
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

    fn get_memory_stats_v1(total_inactive_file: u64) -> MemoryStatsStatsV1 {
        MemoryStatsStatsV1 {
            active_anon: 0,
            active_file: 0,
            cache: 0,
            dirty: 0,
            hierarchical_memory_limit: 0,
            hierarchical_memsw_limit: Some(0),
            inactive_anon: 0,
            inactive_file: 0,
            mapped_file: 0,
            pgfault: 0,
            pgmajfault: 0,
            pgpgin: 0,
            pgpgout: 0,
            rss: 0,
            rss_huge: 0,
            shmem: Some(0),
            total_active_anon: 0,
            total_active_file: 0,
            total_cache: 0,
            total_dirty: 0,
            total_inactive_anon: 0,
            total_inactive_file,
            total_mapped_file: 0,
            total_pgfault: 0,
            total_pgmajfault: 0,
            total_pgpgin: 0,
            total_pgpgout: 0,
            total_rss: 0,
            total_rss_huge: 0,
            total_shmem: Some(0),
            total_unevictable: 0,
            total_writeback: 0,
            unevictable: 0,
            writeback: 0,
        }
    }
}
