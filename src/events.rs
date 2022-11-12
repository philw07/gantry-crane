use tokio::sync::broadcast::{self, Receiver, Sender};

use crate::{constants::BUFFER_SIZE_EVENT_CHANNEL, container::Container, mqtt::MqttMessage};

pub type EventSender = Sender<Event>;
pub type EventReceiver = Receiver<Event>;

pub struct EventChannel {
    #[allow(dead_code)]
    pub tx: EventSender,
}

impl EventChannel {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel::<Event>(BUFFER_SIZE_EVENT_CHANNEL);
        Self { tx }
    }

    pub fn get_receiver(&self) -> EventReceiver {
        self.tx.subscribe()
    }

    pub fn get_sender(&self) -> EventSender {
        self.tx.clone()
    }

    pub fn send(&self, event: Event) {
        if let Err(e) = self.tx.send(event) {
            log::error!("Failed to send event: {}", e);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    ContainerCreated(ContainerEventInfo),
    ContainerRemoved(ContainerEventInfo),
    MqttMessageReceived(MqttMessage),
    PublishMqttMessage(MqttMessage),
    SubscribeMqttTopic(String),
    MqttConnected,
    ForcePoll,
    SuspendPolling,
    ResumePolling,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerEventInfo {
    pub name: String,
    pub is_host_networking: bool,
}

impl From<&Container> for ContainerEventInfo {
    fn from(c: &Container) -> Self {
        Self {
            name: c.get_name().into(),
            is_host_networking: c.is_host_networking,
        }
    }
}

impl From<Container> for ContainerEventInfo {
    fn from(c: Container) -> Self {
        Self {
            name: c.get_name().into(),
            is_host_networking: c.is_host_networking,
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use bollard::{
        container::{
            BlkioStats, CPUStats, CPUUsage, MemoryStats, PidsStats, Stats, StorageStats,
            ThrottlingData,
        },
        service::{ContainerInspectResponse, HostConfig},
    };

    use crate::{container::Container, settings::Settings};

    use super::{ContainerEventInfo, Event, EventChannel};

    #[tokio::test]
    async fn test_event_channel() {
        let channel = EventChannel::new();
        let mut recv = channel.get_receiver();

        assert_eq!(recv.len(), 0);
        channel.send(Event::MqttConnected);
        assert_eq!(recv.len(), 1);
        assert_eq!(recv.recv().await.unwrap(), Event::MqttConnected);

        let sender = channel.get_sender();
        assert_eq!(recv.len(), 0);
        sender.send(Event::MqttConnected).unwrap();
        assert_eq!(recv.len(), 1);
        assert_eq!(recv.recv().await.unwrap(), Event::MqttConnected);
    }

    #[test]
    fn test_container_event_info() {
        let name = "Test Container";

        let settings = Arc::new(Settings::default());
        let event_channel = EventChannel::new();
        let stats = get_stats(&name);
        let inspect = ContainerInspectResponse {
            host_config: Some(HostConfig {
                network_mode: Some("host".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let container = Container::new(settings, &event_channel, &stats, &inspect, None);

        let event_info = ContainerEventInfo::from(&container);
        assert_eq!(event_info.name, container.get_name());
        assert_eq!(event_info.is_host_networking, true);

        let event_info = ContainerEventInfo::from(container);
        assert_eq!(event_info.name, name);
        assert_eq!(event_info.is_host_networking, true);
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
