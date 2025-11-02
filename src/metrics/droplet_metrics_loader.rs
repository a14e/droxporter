use crate::client::do_client::{
    ClientLoadType, DigitalOceanClient, FileSystemRequest, MemoryRequest, NetworkDirection,
    NetworkInterface,
};
use crate::client::do_json_protocol::{
    DropletDataResponse, DropletMetricMetaInfo, DropletMetricsResponse,
};
use crate::config::config_model::{
    AppSettings, BandwidthType, FilesystemTypes, LoadTypes, MemoryTypes,
};
use crate::metrics::droplet_store::DropletStore;
use crate::metrics::utils;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use prometheus::Opts;
use std::sync::Arc;

#[async_trait]
pub trait DropletMetricsService: Send + Sync {
    async fn load_bandwidth(&self) -> anyhow::Result<()>;
    async fn load_cpu_metrics(&self) -> anyhow::Result<()>;
    async fn load_filesystem_metrics(&self) -> anyhow::Result<()>;
    async fn load_memory_metrics(&self) -> anyhow::Result<()>;
    #[allow(dead_code)]
    async fn load_load_metrics(&self) -> anyhow::Result<()>;
}

#[derive(Clone)]
pub struct DropletMetricsServiceImpl {
    client: Arc<dyn DigitalOceanClient>,
    droplet_store: Arc<dyn DropletStore>,
    configs: &'static AppSettings,
    metrics: LoaderDropletMetrics,
}

impl DropletMetricsServiceImpl {
    pub fn new(
        client: Arc<dyn DigitalOceanClient>,
        droplet_store: Arc<dyn DropletStore>,
        configs: &'static AppSettings,
        registry: prometheus::Registry,
    ) -> anyhow::Result<Self> {
        let result = Self {
            client,
            droplet_store,
            configs,
            metrics: LoaderDropletMetrics::new(registry)?,
        };
        Ok(result)
    }
}

#[derive(Clone)]
struct LoaderDropletMetrics {
    droplet_bandwidth: prometheus::GaugeVec,
    droplet_cpu: prometheus::GaugeVec,
    droplet_filesystem: prometheus::GaugeVec,
    droplet_memory: prometheus::GaugeVec,
    #[allow(dead_code)]
    droplet_load: prometheus::GaugeVec,
}

impl LoaderDropletMetrics {
    fn new(registry: prometheus::Registry) -> anyhow::Result<Self> {
        let droplet_bandwidth = prometheus::GaugeVec::new(
            Opts::new("droxporter_droplet_bandwidth", "Bandwidth of droplet"),
            &["droplet", "interface", "direction"],
        )?;
        let droplet_cpu = prometheus::GaugeVec::new(
            Opts::new("droxporter_droplet_cpu", "CPU usage of droplet"),
            &["droplet", "mode"],
        )?;
        let droplet_filesystem = prometheus::GaugeVec::new(
            Opts::new(
                "droxporter_droplet_filesystem",
                "Filesystem usage of droplet",
            ),
            &["droplet", "metric_type", "device", "fstype", "mountpoint"],
        )?;
        let droplet_memory = prometheus::GaugeVec::new(
            Opts::new("droxporter_droplet_memory", "Memory usage of droplet"),
            &["droplet", "metric_type"],
        )?;
        let droplet_load = prometheus::GaugeVec::new(
            Opts::new("droxporter_droplet_load", "Load of droplet"),
            &["droplet", "metric_type"],
        )?;
        registry.register(Box::new(droplet_bandwidth.clone()))?;
        registry.register(Box::new(droplet_cpu.clone()))?;
        registry.register(Box::new(droplet_filesystem.clone()))?;
        registry.register(Box::new(droplet_memory.clone()))?;
        registry.register(Box::new(droplet_load.clone()))?;
        let result = Self {
            droplet_bandwidth,
            droplet_cpu,
            droplet_filesystem,
            droplet_memory,
            droplet_load,
        };
        Ok(result)
    }
}

macro_rules! unwrap_or_return_ok {
    ($block:expr) => {
        match $block {
            None => return Ok(()),
            Some(x) => x,
        }
    };
}

fn extract_last_value(response: DropletDataResponse) -> f64 {
    response
        .data
        .result
        .iter()
        .flat_map(|x| x.values.iter())
        .max_by_key(|x| x.timestamp)
        .and_then(|x| x.value.parse::<f64>().ok())
        .unwrap_or(0f64)
}

fn extract_meta_with_last_values(
    response: DropletDataResponse,
) -> Vec<(DropletMetricMetaInfo, f64)> {
    response
        .data
        .result
        .into_iter()
        .map(|x| {
            let last_point = last_point_for_metric(&x);
            let info = x.metric;
            (info, last_point)
        })
        .collect()
}

fn last_point_for_metric(metrics: &DropletMetricsResponse) -> f64 {
    metrics
        .values
        .iter()
        .max_by_key(|x| x.timestamp)
        .and_then(|x| x.value.parse::<f64>().ok())
        .unwrap_or(0f64)
}

fn metrics_read_interval() -> Duration {
    // It seems that DO has a 10..15 second interval between points, so I think an interval of 1 minute is reasonable.
    Duration::minutes(1)
}

// a lot of boilerplate. but I don't think it would be changing too often
#[async_trait]
impl DropletMetricsService for DropletMetricsServiceImpl {
    async fn load_bandwidth(&self) -> anyhow::Result<()> {
        let bandwidth = unwrap_or_return_ok!(self.configs.droplet_metrics.bandwidth.as_ref());

        let enable_private_in = bandwidth.types.contains(&BandwidthType::PrivateInbound);
        let enable_private_out = bandwidth.types.contains(&BandwidthType::PrivateOutbound);
        let enable_public_in = bandwidth.types.contains(&BandwidthType::PublicInbound);
        let enable_public_out = bandwidth.types.contains(&BandwidthType::PublicOutbound);

        let metric_types: Vec<_> = [
            (
                enable_private_in,
                NetworkInterface::Private,
                NetworkDirection::Inbound,
            ),
            (
                enable_private_out,
                NetworkInterface::Private,
                NetworkDirection::Outbound,
            ),
            (
                enable_public_in,
                NetworkInterface::Public,
                NetworkDirection::Inbound,
            ),
            (
                enable_public_out,
                NetworkInterface::Public,
                NetworkDirection::Outbound,
            ),
        ]
        .iter()
        .filter(|(enabled, _, _)| *enabled)
        .map(|(_, interface, dir)| (*interface, *dir))
        .collect();

        let interval_end = Utc::now();
        let interval_start = interval_end - metrics_read_interval();

        let droplets = self.droplet_store.list_droplets();
        for droplet in droplets.iter() {
            for (interface, dir) in &metric_types {
                let res = self
                    .client
                    .get_droplet_bandwidth(
                        droplet.id,
                        *interface,
                        *dir,
                        interval_start,
                        interval_end,
                    )
                    .await?;
                let value = extract_last_value(res);
                let interface = match interface {
                    NetworkInterface::Public => "public",
                    NetworkInterface::Private => "private",
                };
                let direction = match dir {
                    NetworkDirection::Inbound => "inbound",
                    NetworkDirection::Outbound => "outbound",
                };

                self.metrics
                    .droplet_bandwidth
                    .with_label_values(&[droplet.name.as_str(), interface, direction])
                    .set(value);
            }
        }

        let droplets = self.droplet_store.list_droplets();
        let droplets_names: ahash::HashSet<_> = droplets.iter().map(|x| x.name.as_str()).collect();
        utils::remove_old_droplets(&self.metrics.droplet_bandwidth, &droplets_names);

        Ok(())
    }

    async fn load_cpu_metrics(&self) -> anyhow::Result<()> {
        let interval_end = Utc::now();
        let interval_start = interval_end - metrics_read_interval();

        for droplet in self.droplet_store.list_droplets().iter() {
            let res = self
                .client
                .get_droplet_cpu(droplet.id, interval_start, interval_end)
                .await?;
            for (meta, value) in extract_meta_with_last_values(res) {
                let mode = meta.mode.as_deref().unwrap_or("unknown");
                self.metrics
                    .droplet_cpu
                    .with_label_values(&[droplet.name.as_str(), mode])
                    .set(value);
            }
        }

        let droplets = self.droplet_store.list_droplets();
        let droplets_names: ahash::HashSet<_> = droplets.iter().map(|x| x.name.as_str()).collect();
        utils::remove_old_droplets(&self.metrics.droplet_cpu, &droplets_names);

        Ok(())
    }

    async fn load_filesystem_metrics(&self) -> anyhow::Result<()> {
        let filesystem = unwrap_or_return_ok!(self.configs.droplet_metrics.filesystem.as_ref());

        let enable_free = filesystem.types.contains(&FilesystemTypes::Free);
        let enable_size = filesystem.types.contains(&FilesystemTypes::Size);

        let filesystem_types: Vec<_> = [
            (enable_free, FileSystemRequest::Free),
            (enable_size, FileSystemRequest::Size),
        ]
        .iter()
        .filter(|(enabled, _)| *enabled)
        .map(|(_, client_type)| *client_type)
        .collect();

        let interval_end = Utc::now();
        let interval_start = interval_end - metrics_read_interval();

        for droplet in self.droplet_store.list_droplets().iter() {
            for metrics_type in &filesystem_types {
                let res = self
                    .client
                    .get_droplet_file_system(
                        droplet.id,
                        *metrics_type,
                        interval_start,
                        interval_end,
                    )
                    .await?;

                let fs_metrics_type_str = match metrics_type {
                    FileSystemRequest::Free => "free",
                    FileSystemRequest::Size => "size",
                };
                for (meta, value) in extract_meta_with_last_values(res) {
                    let device = meta.device.as_deref().unwrap_or("unknown");
                    let fstype = meta.fstype.as_deref().unwrap_or("unknown");
                    let mountpoint = meta.mountpoint.as_deref().unwrap_or("unknown");

                    self.metrics
                        .droplet_filesystem
                        .with_label_values(&[
                            droplet.name.as_str(),
                            fs_metrics_type_str,
                            device,
                            fstype,
                            mountpoint,
                        ])
                        .set(value);
                }
            }
        }

        let droplets = self.droplet_store.list_droplets();
        let droplets_names: ahash::HashSet<_> = droplets.iter().map(|x| x.name.as_str()).collect();
        utils::remove_old_droplets(&self.metrics.droplet_filesystem, &droplets_names);

        Ok(())
    }

    async fn load_memory_metrics(&self) -> anyhow::Result<()> {
        let memory = unwrap_or_return_ok!(self.configs.droplet_metrics.memory.as_ref());

        let enable_free = memory.types.contains(&MemoryTypes::Free);
        let enable_available = memory.types.contains(&MemoryTypes::Available);
        let enable_cached = memory.types.contains(&MemoryTypes::Cached);
        let enable_total = memory.types.contains(&MemoryTypes::Total);

        let memory_types: Vec<_> = [
            (enable_free, MemoryRequest::Free),
            (enable_available, MemoryRequest::AvailableTotal),
            (enable_cached, MemoryRequest::Cached),
            (enable_total, MemoryRequest::Total),
        ]
        .iter()
        .filter(|(enabled, _)| *enabled)
        .map(|(_, client_type)| *client_type)
        .collect();

        let interval_end = Utc::now();
        let interval_start = interval_end - Duration::minutes(30);

        for droplet in self.droplet_store.list_droplets().iter() {
            for memory_type in &memory_types {
                let res = self
                    .client
                    .get_droplet_memory(droplet.id, *memory_type, interval_start, interval_end)
                    .await?;
                let value = extract_last_value(res);

                let memory_type_str = match memory_type {
                    MemoryRequest::Cached => "cached",
                    MemoryRequest::Free => "free",
                    MemoryRequest::Total => "total",
                    MemoryRequest::AvailableTotal => "available",
                };

                self.metrics
                    .droplet_memory
                    .with_label_values(&[droplet.name.as_str(), memory_type_str])
                    .set(value);
            }
        }

        let droplets = self.droplet_store.list_droplets();
        let droplets_names: ahash::HashSet<_> = droplets.iter().map(|x| x.name.as_str()).collect();
        utils::remove_old_droplets(&self.metrics.droplet_memory, &droplets_names);

        Ok(())
    }

    async fn load_load_metrics(&self) -> anyhow::Result<()> {
        let load = unwrap_or_return_ok!(self.configs.droplet_metrics.load.as_ref());

        let enable_load1 = load.types.contains(&LoadTypes::Load1);
        let enable_load5 = load.types.contains(&LoadTypes::Load5);
        let enable_load15 = load.types.contains(&LoadTypes::Load15);

        let load_types: Vec<_> = [
            (enable_load1, ClientLoadType::Load1),
            (enable_load5, ClientLoadType::Load5),
            (enable_load15, ClientLoadType::Load15),
        ]
        .iter()
        .filter(|(enabled, _)| *enabled)
        .map(|(_, client_type)| *client_type)
        .collect();

        let interval_end = Utc::now();
        let interval_start = interval_end - Duration::minutes(30);

        for droplet in self.droplet_store.list_droplets().iter() {
            for load_type in &load_types {
                let res = self
                    .client
                    .get_droplet_load(droplet.id, *load_type, interval_start, interval_end)
                    .await?;
                let value = extract_last_value(res);

                let load_type_str = match load_type {
                    ClientLoadType::Load1 => "load_1",
                    ClientLoadType::Load5 => "load_5",
                    ClientLoadType::Load15 => "load_15",
                };

                self.metrics
                    .droplet_load
                    .with_label_values(&[droplet.name.as_str(), load_type_str])
                    .set(value);
            }
        }

        let droplets = self.droplet_store.list_droplets();
        let droplets_names: ahash::HashSet<_> = droplets.iter().map(|x| x.name.as_str()).collect();
        utils::remove_old_droplets(&self.metrics.droplet_load, &droplets_names);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::do_client::MockDigitalOceanClient;
    use crate::client::do_json_protocol::{
        DropletDataResponse, DropletDataResult, DropletMetricMetaInfo, DropletMetricsResponse,
        MetricPoint,
    };
    use crate::config::config_model::{
        AppSettings, BandwidthSettings, BandwidthType, CpuSettings, FilesystemSettings,
        FilesystemTypes, MemorySettings, MemoryTypes,
    };
    use crate::metrics::droplet_store::{BasicDropletInfo, MockDropletStore};
    use std::time::Duration as StdDuration;

    fn create_test_config() -> &'static AppSettings {
        let config = AppSettings {
            default_keys: vec!["test-key".to_string()],
            droplets: crate::config::config_model::DropletSettings {
                keys: vec![],
                url: "http://test.com/droplets".to_string(),
                interval: StdDuration::from_secs(60),
                metrics: vec![],
            },
            apps: crate::config::config_model::AppPlatformSettings {
                keys: vec![],
                url: "http://test.com/apps".to_string(),
                interval: StdDuration::from_secs(60),
                metrics: vec![],
            },
            droplet_metrics: crate::config::config_model::DropletMetricsConfig {
                base_url: "http://test.com/metrics".to_string(),
                bandwidth: Some(BandwidthSettings {
                    enabled: true,
                    interval: StdDuration::from_secs(60),
                    types: vec![BandwidthType::PublicInbound],
                    keys: vec![],
                }),
                cpu: Some(CpuSettings {
                    enabled: true,
                    interval: StdDuration::from_secs(60),
                    keys: vec![],
                }),
                filesystem: Some(FilesystemSettings {
                    enabled: true,
                    interval: StdDuration::from_secs(60),
                    types: vec![FilesystemTypes::Free],
                    keys: vec![],
                }),
                memory: Some(MemorySettings {
                    enabled: false,
                    interval: StdDuration::from_secs(60),
                    types: vec![MemoryTypes::Free],
                    keys: vec![],
                }),
                load: None,
            },
            app_metrics: crate::config::config_model::AppMetricsConfig {
                base_url: "http://test.com/app_metrics".to_string(),
                cpu_percentage: None,
                memory_percentage: None,
                restart_count: None,
            },
            exporter_metrics: crate::config::config_model::ExporterMetricsConfigs {
                enabled: false,
                interval: StdDuration::from_secs(60),
                metrics: vec![],
            },
            endpoint: crate::config::config_model::EndpointConfig {
                port: 8888,
                host: "0.0.0.0".to_string(),
                auth: None,
                ssl: None,
            },
            custom: crate::config::config_model::CustomSettings {
                prefix: None,
                labels: std::collections::HashMap::new(),
            },
        };
        Box::leak(Box::new(config))
    }

    #[tokio::test]
    async fn test_load_cpu_metrics_success() {
        let mut mock_client = MockDigitalOceanClient::new();
        let mut mock_store = MockDropletStore::new();

        let droplets = vec![BasicDropletInfo {
            id: 123,
            name: "test-droplet".to_string(),
            memory: 2048,
            vcpus: 2,
            disk: 50,
            locked: false,
            status: "active".to_string(),
        }];

        mock_store
            .expect_list_droplets()
            .times(2)
            .returning(move || droplets.clone());

        mock_client
            .expect_get_droplet_cpu()
            .withf(|id, _start, _end| *id == 123)
            .times(1)
            .returning(|_, _, _| {
                Ok(DropletDataResponse {
                    status: "success".to_string(),
                    data: DropletDataResult {
                        result: vec![DropletMetricsResponse {
                            metric: DropletMetricMetaInfo {
                                host_id: "123".to_string(),
                                mode: Some("idle".to_string()),
                                ..Default::default()
                            },
                            values: vec![MetricPoint {
                                timestamp: 1682246520,
                                value: "95.5".to_string(),
                            }],
                        }],
                    },
                })
            });

        let config = create_test_config();
        let registry = prometheus::Registry::new();

        let service = DropletMetricsServiceImpl::new(
            Arc::new(mock_client),
            Arc::new(mock_store),
            config,
            registry,
        )
        .unwrap();

        let result = service.load_cpu_metrics().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_load_bandwidth_success() {
        let mut mock_client = MockDigitalOceanClient::new();
        let mut mock_store = MockDropletStore::new();

        let droplets = vec![BasicDropletInfo {
            id: 456,
            name: "test-droplet-2".to_string(),
            memory: 4096,
            vcpus: 4,
            disk: 100,
            locked: false,
            status: "active".to_string(),
        }];

        mock_store
            .expect_list_droplets()
            .times(2)
            .returning(move || droplets.clone());

        mock_client
            .expect_get_droplet_bandwidth()
            .withf(|id, interface, direction, _start, _end| {
                *id == 456
                    && *interface == NetworkInterface::Public
                    && *direction == NetworkDirection::Inbound
            })
            .times(1)
            .returning(|_, _, _, _, _| {
                Ok(DropletDataResponse {
                    status: "success".to_string(),
                    data: DropletDataResult {
                        result: vec![DropletMetricsResponse {
                            metric: DropletMetricMetaInfo {
                                host_id: "456".to_string(),
                                ..Default::default()
                            },
                            values: vec![MetricPoint {
                                timestamp: 1682246520,
                                value: "1024.5".to_string(),
                            }],
                        }],
                    },
                })
            });

        let config = create_test_config();
        let registry = prometheus::Registry::new();

        let service = DropletMetricsServiceImpl::new(
            Arc::new(mock_client),
            Arc::new(mock_store),
            config,
            registry,
        )
        .unwrap();

        let result = service.load_bandwidth().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_load_memory_metrics_success() {
        let mut mock_client = MockDigitalOceanClient::new();
        let mut mock_store = MockDropletStore::new();

        let droplets = vec![BasicDropletInfo {
            id: 789,
            name: "test-droplet-3".to_string(),
            memory: 8192,
            vcpus: 8,
            disk: 200,
            locked: false,
            status: "active".to_string(),
        }];

        mock_store
            .expect_list_droplets()
            .times(2)
            .returning(move || droplets.clone());

        mock_client
            .expect_get_droplet_memory()
            .withf(|id, metric_type, _start, _end| {
                *id == 789 && matches!(metric_type, MemoryRequest::Free)
            })
            .times(1)
            .returning(|_, _, _, _| {
                Ok(DropletDataResponse {
                    status: "success".to_string(),
                    data: DropletDataResult {
                        result: vec![DropletMetricsResponse {
                            metric: DropletMetricMetaInfo {
                                host_id: "789".to_string(),
                                ..Default::default()
                            },
                            values: vec![MetricPoint {
                                timestamp: 1682246520,
                                value: "512000000".to_string(),
                            }],
                        }],
                    },
                })
            });

        let config = create_test_config();
        let registry = prometheus::Registry::new();

        let service = DropletMetricsServiceImpl::new(
            Arc::new(mock_client),
            Arc::new(mock_store),
            config,
            registry,
        )
        .unwrap();

        let result = service.load_memory_metrics().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_load_filesystem_metrics_success() {
        let mut mock_client = MockDigitalOceanClient::new();
        let mut mock_store = MockDropletStore::new();

        let droplets = vec![BasicDropletInfo {
            id: 999,
            name: "test-droplet-4".to_string(),
            memory: 1024,
            vcpus: 1,
            disk: 25,
            locked: false,
            status: "active".to_string(),
        }];

        mock_store
            .expect_list_droplets()
            .times(2)
            .returning(move || droplets.clone());

        mock_client
            .expect_get_droplet_file_system()
            .withf(|id, fs_type, _start, _end| {
                *id == 999 && matches!(fs_type, FileSystemRequest::Free)
            })
            .times(1)
            .returning(|_, _, _, _| {
                Ok(DropletDataResponse {
                    status: "success".to_string(),
                    data: DropletDataResult {
                        result: vec![DropletMetricsResponse {
                            metric: DropletMetricMetaInfo {
                                host_id: "999".to_string(),
                                device: Some("/dev/vda1".to_string()),
                                fstype: Some("ext4".to_string()),
                                mountpoint: Some("/".to_string()),
                                ..Default::default()
                            },
                            values: vec![MetricPoint {
                                timestamp: 1682246520,
                                value: "10000000000".to_string(),
                            }],
                        }],
                    },
                })
            });

        let config = create_test_config();
        let registry = prometheus::Registry::new();

        let service = DropletMetricsServiceImpl::new(
            Arc::new(mock_client),
            Arc::new(mock_store),
            config,
            registry,
        )
        .unwrap();

        let result = service.load_filesystem_metrics().await;
        assert!(result.is_ok());
    }
}
