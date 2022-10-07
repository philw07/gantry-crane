use std::sync::Arc;

use serde::Serialize;

use bollard::{
    container::{MemoryStatsStats, Stats},
    service::{ContainerInspectResponse, ContainerStateStatusEnum, HealthStatusEnum},
};

use crate::{
    constants::{PRECISION, UNKNOWN},
    events::{Event, EventChannel, EventSender},
    mqtt::MqttMessage,
    settings::Settings,
    util::round,
};

#[derive(Clone, Serialize)]
pub struct Container {
    #[serde(skip)]
    settings: Arc<Settings>,
    #[serde(skip)]
    event_tx: EventSender,

    #[serde(serialize_with = "serialize_name")]
    name: String,
    image: String,

    state: String,
    health: String,

    cpu_percentage: f64,
    mem_percentage: f64,
    mem_mb: f64,
    net_rx_mb: f64,
    net_tx_mb: f64,
    block_rx_mb: f64,
    block_tx_mb: f64,
}

impl Container {
    pub fn new(
        settings: Arc<Settings>,
        event_channel: &EventChannel,
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
            settings,
            event_tx: event_channel.get_sender(),

            name: stats.name.clone(),
            image: image.unwrap_or_else(|| UNKNOWN.into()),

            state: UNKNOWN.into(),
            health: UNKNOWN.into(),

            cpu_percentage: 0.0,
            mem_percentage: 0.0,
            mem_mb: 0.0,
            net_rx_mb: 0.0,
            net_tx_mb: 0.0,
            block_rx_mb: 0.0,
            block_tx_mb: 0.0,
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
            self.parse_network(&stats);
            self.parse_block_io(&stats);
        }
    }

    pub async fn publish(&self) {
        match serde_json::to_string(&self) {
            Ok(json) => {
                let msg = MqttMessage::new(self.get_topic(), json, true, 0);
                if let Err(e) = self.event_tx.send(Event::PublishMqttMessage(msg)) {
                    log::error!("Failed to publish container '{}': {}", self.name, e);
                }
            }
            Err(e) => {
                log::error!("Failed to serialize container '{}': {}", self.name, e)
            }
        };
    }

    pub async fn unpublish(&self) {
        let msg = MqttMessage::new(self.get_topic(), "".into(), true, 1);
        if let Err(e) = self.event_tx.send(Event::PublishMqttMessage(msg)) {
            log::error!("Failed to unpublish container '{}': {}", self.name, e);
        }
    }

    fn get_topic(&self) -> String {
        format!("{}/{}", self.settings.mqtt.base_topic, &self.name[1..])
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

        self.mem_mb = round(mem, PRECISION);
        self.mem_percentage = round(percentage, PRECISION);
    }

    fn parse_network(&mut self, stats: &Stats) {
        let mut rx = 0.0;
        let mut tx = 0.0;

        // Sum all networks
        if let Some(networks) = stats.networks.as_ref() {
            for network in networks.values() {
                rx += network.rx_bytes as f64;
                tx += network.tx_bytes as f64;
            }
        }

        // Convert to MiB
        rx /= 1_048_576.0;
        tx /= 1_048_576.0;

        self.net_rx_mb = round(rx, PRECISION);
        self.net_tx_mb = round(tx, PRECISION);
    }

    fn parse_block_io(&mut self, stats: &Stats) {
        let mut rx = 0.0;
        let mut tx = 0.0;

        // Sum all entries
        if let Some(entries) = stats.blkio_stats.io_service_bytes_recursive.as_ref() {
            for entry in entries {
                match entry.op.to_lowercase().as_str() {
                    "read" => rx += entry.value as f64,
                    "write" => tx += entry.value as f64,
                    _ => (),
                }
            }
        }

        // Convert to MiB
        rx /= 1_048_576.0;
        tx /= 1_048_576.0;

        self.block_rx_mb = round(rx, PRECISION);
        self.block_tx_mb = round(tx, PRECISION);
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
    use std::{collections::HashMap, sync::Arc};

    use bollard::{
        container::{
            BlkioStats, BlkioStatsEntry, CPUStats, CPUUsage, MemoryStats, MemoryStatsStats,
            MemoryStatsStatsV1, NetworkStats, PidsStats, Stats, StorageStats, ThrottlingData,
        },
        service::{
            ContainerInspectResponse, ContainerState, ContainerStateStatusEnum, Health,
            HealthStatusEnum,
        },
    };

    use crate::{
        constants::UNKNOWN,
        events::{Event, EventChannel},
        mqtt::MqttMessage,
        settings::Settings,
    };

    use super::Container;

    #[test]
    fn test_new_container() {
        let name = "test_name";
        let image = "test-image";
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();

        let stats = get_stats(name);
        let inspect = ContainerInspectResponse {
            name: Some(name.into()),
            ..Default::default()
        };

        let container =
            Container::new(settings, &event_channel, stats, inspect, Some(image.into()));

        assert_eq!(container.name, name);
        assert_eq!(container.image, image);
    }

    #[test]
    fn test_new_container_no_image() {
        let name = "other_name";
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();

        let stats = get_stats(name);
        let inspect = ContainerInspectResponse {
            name: Some(name.into()),
            ..Default::default()
        };

        let container = Container::new(settings, &event_channel, stats, inspect, None);

        assert_eq!(container.name, name);
        assert_eq!(container.image, UNKNOWN.to_owned());
    }

    #[test]
    fn test_rename() {
        let name = "original_name";
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();

        let stats = get_stats(name);
        let inspect = ContainerInspectResponse {
            name: Some(name.into()),
            ..Default::default()
        };

        let mut container = Container::new(settings, &event_channel, stats, inspect, None);
        assert_eq!(container.name, name);

        let new_name = "renamed_name";
        container.rename(new_name.into());
        assert_eq!(container.name, new_name);
    }

    #[test]
    fn test_update() {
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();
        let mut stats = get_stats("");
        let mut inspect = ContainerInspectResponse {
            ..Default::default()
        };

        let mut container = Container::new(
            settings,
            &event_channel,
            stats.clone(),
            inspect.clone(),
            None,
        );

        // Update state and health
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

        // Update CPU
        stats.cpu_stats.cpu_usage.total_usage = 20;
        stats.cpu_stats.system_cpu_usage = Some(200);
        stats.precpu_stats.cpu_usage.total_usage = 10;
        stats.precpu_stats.system_cpu_usage = Some(100);
        stats.cpu_stats.cpu_usage.percpu_usage = Some(vec![0; 2]);

        // Update memory
        stats.memory_stats.usage = Some(100 * 1_048_576);
        stats.memory_stats.limit = Some(1000 * 1_048_576);

        // Update network I/O
        stats.networks = Some(HashMap::new());
        stats
            .networks
            .as_mut()
            .unwrap()
            .insert("a".into(), get_network_stats(5 * 1_048_576, 10 * 1_048_576));

        // Update block I/O
        stats.blkio_stats.io_service_bytes_recursive = Some(Vec::new());
        stats
            .blkio_stats
            .io_service_bytes_recursive
            .as_mut()
            .unwrap()
            .insert(
                0,
                BlkioStatsEntry {
                    major: 0,
                    minor: 0,
                    op: "Read".into(),
                    value: 10 * 1_048_576,
                },
            );
        stats
            .blkio_stats
            .io_service_bytes_recursive
            .as_mut()
            .unwrap()
            .insert(
                0,
                BlkioStatsEntry {
                    major: 0,
                    minor: 0,
                    op: "Write".into(),
                    value: 3 * 1_048_576,
                },
            );

        container.update(stats, inspect);
        assert_eq!(container.state, status.as_ref());
        assert_eq!(container.health, health.as_ref());
        assert_eq!(container.cpu_percentage, 20.0);
        assert_eq!(container.mem_mb, 100.0);
        assert_eq!(container.mem_percentage, 10.0);
        assert_eq!(container.net_rx_mb, 5.0);
        assert_eq!(container.net_tx_mb, 10.0);
        assert_eq!(container.block_rx_mb, 10.0);
        assert_eq!(container.block_tx_mb, 3.0);
    }

    #[test]
    fn test_parse_state() {
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();
        let stats = get_stats("");
        let mut inspect = ContainerInspectResponse {
            ..Default::default()
        };
        let mut container = Container::new(
            settings,
            &event_channel,
            stats.clone(),
            inspect.clone(),
            None,
        );

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
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();
        let stats = get_stats("");
        let mut inspect = ContainerInspectResponse {
            ..Default::default()
        };
        let mut container = Container::new(
            settings,
            &event_channel,
            stats.clone(),
            inspect.clone(),
            None,
        );

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
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();
        let mut stats = get_stats("");
        let inspect = ContainerInspectResponse {
            ..Default::default()
        };
        let mut container = Container::new(
            settings,
            &event_channel,
            stats.clone(),
            inspect.clone(),
            None,
        );

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
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();
        let mut stats = get_stats("");
        let inspect = ContainerInspectResponse {
            ..Default::default()
        };
        let mut container = Container::new(
            settings,
            &event_channel,
            stats.clone(),
            inspect.clone(),
            None,
        );

        assert_eq!(container.mem_mb, 0.0);
        assert_eq!(container.mem_percentage, 0.0);

        stats.memory_stats.usage = Some(100 * 1_048_576);
        container.parse_mem(&stats);
        assert_eq!(container.mem_mb, 100.0);
        assert_eq!(container.mem_percentage, 0.0);

        stats.memory_stats.limit = Some(1000 * 1_048_576);
        container.parse_mem(&stats);
        assert_eq!(container.mem_mb, 100.0);
        assert_eq!(container.mem_percentage, 10.0);

        stats.memory_stats.stats = Some(MemoryStatsStats::V1(get_memory_stats_v1(10 * 1_048_576)));
        container.parse_mem(&stats);
        assert_eq!(container.mem_mb, 90.0);
        assert_eq!(container.mem_percentage, 9.0);
    }

    #[test]
    fn test_parse_network() {
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();
        let mut stats = get_stats("");
        let inspect = ContainerInspectResponse {
            ..Default::default()
        };
        let mut container = Container::new(
            settings,
            &event_channel,
            stats.clone(),
            inspect.clone(),
            None,
        );

        assert_eq!(container.net_rx_mb, 0.0);
        assert_eq!(container.net_tx_mb, 0.0);

        stats.networks = Some(HashMap::new());
        assert_eq!(container.net_rx_mb, 0.0);
        assert_eq!(container.net_tx_mb, 0.0);

        stats
            .networks
            .as_mut()
            .unwrap()
            .insert("a".into(), get_network_stats(5 * 1_048_576, 10 * 1_048_576));
        container.parse_network(&stats);
        assert_eq!(container.net_rx_mb, 5.0);
        assert_eq!(container.net_tx_mb, 10.0);

        stats
            .networks
            .as_mut()
            .unwrap()
            .insert("b".into(), get_network_stats(7 * 1_048_576, 15 * 1_048_576));
        stats
            .networks
            .as_mut()
            .unwrap()
            .insert("c".into(), get_network_stats(8 * 1_048_576, 3 * 1_048_576));
        container.parse_network(&stats);
        assert_eq!(container.net_rx_mb, 20.0);
        assert_eq!(container.net_tx_mb, 28.0);
    }

    #[test]
    fn test_parse_block_io() {
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();
        let mut stats = get_stats("");
        let inspect = ContainerInspectResponse {
            ..Default::default()
        };
        let mut container = Container::new(
            settings,
            &event_channel,
            stats.clone(),
            inspect.clone(),
            None,
        );

        assert_eq!(container.block_rx_mb, 0.0);
        assert_eq!(container.block_tx_mb, 0.0);

        stats.blkio_stats.io_service_bytes_recursive = Some(Vec::new());
        container.parse_block_io(&stats);
        assert_eq!(container.block_rx_mb, 0.0);
        assert_eq!(container.block_tx_mb, 0.0);

        stats
            .blkio_stats
            .io_service_bytes_recursive
            .as_mut()
            .unwrap()
            .insert(
                0,
                BlkioStatsEntry {
                    major: 0,
                    minor: 0,
                    op: "Read".into(),
                    value: 10 * 1_048_576,
                },
            );
        stats
            .blkio_stats
            .io_service_bytes_recursive
            .as_mut()
            .unwrap()
            .insert(
                0,
                BlkioStatsEntry {
                    major: 0,
                    minor: 0,
                    op: "Write".into(),
                    value: 3 * 1_048_576,
                },
            );
        stats
            .blkio_stats
            .io_service_bytes_recursive
            .as_mut()
            .unwrap()
            .insert(
                0,
                BlkioStatsEntry {
                    major: 0,
                    minor: 0,
                    op: "Write".into(),
                    value: 4 * 1_048_576,
                },
            );
        stats
            .blkio_stats
            .io_service_bytes_recursive
            .as_mut()
            .unwrap()
            .insert(
                0,
                BlkioStatsEntry {
                    major: 0,
                    minor: 0,
                    op: "Read".into(),
                    value: 5 * 1_048_576,
                },
            );
        stats
            .blkio_stats
            .io_service_bytes_recursive
            .as_mut()
            .unwrap()
            .insert(
                0,
                BlkioStatsEntry {
                    major: 0,
                    minor: 0,
                    op: "Write".into(),
                    value: 3 * 1_048_576,
                },
            );
        container.parse_block_io(&stats);
        assert_eq!(container.block_rx_mb, 15.0);
        assert_eq!(container.block_tx_mb, 10.0);
    }

    #[test]
    fn test_get_topic() {
        let name = "/test_name";
        let image = "test-image";
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();

        let stats = get_stats(name);
        let inspect = ContainerInspectResponse {
            name: Some(name.into()),
            ..Default::default()
        };

        let container = Container::new(
            settings.clone(),
            &event_channel,
            stats,
            inspect,
            Some(image.into()),
        );
        let topic = container.get_topic();
        assert_eq!(topic.matches('/').count(), 1);
        assert_eq!(
            topic.split('/').collect::<Vec<_>>(),
            [&settings.mqtt.base_topic, &name[1..]]
        );
    }

    #[tokio::test]
    async fn test_publish() {
        let name = "test_name";
        let image = "test-image";
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();

        let stats = get_stats(name);
        let inspect = ContainerInspectResponse {
            name: Some(name.into()),
            ..Default::default()
        };

        let container =
            Container::new(settings, &event_channel, stats, inspect, Some(image.into()));

        let mut recv = event_channel.get_receiver();
        container.publish().await;
        assert_eq!(recv.len(), 1);
        let expected_msg = MqttMessage::new(
            container.get_topic().into(),
            serde_json::to_string(&container).unwrap().into(),
            true,
            0,
        );
        assert_eq!(
            recv.recv().await,
            Ok(Event::PublishMqttMessage(expected_msg))
        );
    }

    #[tokio::test]
    async fn test_unpublish() {
        let name = "test_name";
        let image = "test-image";
        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();

        let stats = get_stats(name);
        let inspect = ContainerInspectResponse {
            name: Some(name.into()),
            ..Default::default()
        };

        let container =
            Container::new(settings, &event_channel, stats, inspect, Some(image.into()));

        let mut recv = event_channel.get_receiver();
        container.unpublish().await;
        assert_eq!(recv.len(), 1);
        let expected_msg = MqttMessage::new(container.get_topic().into(), "".into(), true, 1);
        assert_eq!(
            recv.recv().await,
            Ok(Event::PublishMqttMessage(expected_msg))
        );
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

    fn get_network_stats(rx: u64, tx: u64) -> NetworkStats {
        NetworkStats {
            rx_dropped: 0,
            rx_bytes: rx,
            rx_errors: 0,
            tx_packets: 0,
            tx_dropped: 0,
            rx_packets: 0,
            tx_errors: 0,
            tx_bytes: tx,
        }
    }
}
