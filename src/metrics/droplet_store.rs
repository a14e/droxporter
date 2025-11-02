use crate::client::do_client::DigitalOceanClient;
use crate::client::do_json_protocol::DropletResponse;
use crate::config::config_model::{AppSettings, DropletMetricsTypes};
use crate::metrics::utils;
use ahash::HashSet;
use async_trait::async_trait;
use parking_lot::RwLock;
use prometheus::Opts;
use std::sync::Arc;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait DropletStore: Send + Sync {
    async fn load_droplets(&self) -> anyhow::Result<()>;

    fn record_droplets_metrics(&self);

    fn list_droplets(&self) -> Vec<BasicDropletInfo>;
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct BasicDropletInfo {
    pub id: u64,
    pub name: String,
    pub memory: u64,
    pub vcpus: u64,
    pub disk: u64,
    pub locked: bool,
    pub status: String,
}

impl From<DropletResponse> for BasicDropletInfo {
    fn from(value: DropletResponse) -> Self {
        Self {
            id: value.id,
            name: value.name,
            memory: value.memory,
            vcpus: value.vcpus,
            disk: value.disk,
            locked: value.locked,
            status: value.status,
        }
    }
}

#[derive(Clone)]
pub struct DropletStoreImpl {
    store: Arc<RwLock<Vec<BasicDropletInfo>>>,
    client: Arc<dyn DigitalOceanClient>,
    configs: &'static AppSettings,
    metrics: DropletsMetrics,
}

impl DropletStoreImpl {
    pub fn new(
        client: Arc<dyn DigitalOceanClient>,
        configs: &'static AppSettings,
        registry: prometheus::Registry,
    ) -> anyhow::Result<Self> {
        let result = Self {
            store: Arc::new(RwLock::new(vec![])),
            client,
            configs,
            metrics: DropletsMetrics::new(registry)?,
        };
        Ok(result)
    }
}

#[derive(Clone)]
struct DropletsMetrics {
    memory_gauge: prometheus::GaugeVec,
    vcpu_gauge: prometheus::GaugeVec,
    disk_gauge: prometheus::GaugeVec,
    status_gauge: prometheus::GaugeVec,
}

impl DropletsMetrics {
    fn new(registry: prometheus::Registry) -> anyhow::Result<Self> {
        let memory_gauge = prometheus::GaugeVec::new(
            Opts::new(
                "droxporter_droplet_memory_settings",
                "Memory settings of droplet",
            ),
            &["droplet"],
        )?;
        let vcpu_gauge = prometheus::GaugeVec::new(
            Opts::new(
                "droxporter_droplet_vcpu_settings",
                "Cpu settings of droplet",
            ),
            &["droplet"],
        )?;
        let disk_gauge = prometheus::GaugeVec::new(
            Opts::new(
                "droxporter_droplet_disk_settings",
                "Disk settings of droplet",
            ),
            &["droplet"],
        )?;
        let status_gauge = prometheus::GaugeVec::new(
            Opts::new("droxporter_droplet_status", "Status of droplet"),
            &["droplet", "status"],
        )?;

        registry.register(Box::new(memory_gauge.clone()))?;
        registry.register(Box::new(vcpu_gauge.clone()))?;
        registry.register(Box::new(disk_gauge.clone()))?;
        registry.register(Box::new(status_gauge.clone()))?;

        let result = Self {
            memory_gauge,
            vcpu_gauge,
            disk_gauge,
            status_gauge,
        };
        Ok(result)
    }
}

impl DropletStoreImpl {
    fn save_droplets(&self, droplets: Vec<BasicDropletInfo>) {
        *self.store.write() = droplets;
    }
}

#[async_trait]
impl DropletStore for DropletStoreImpl {
    async fn load_droplets(&self) -> anyhow::Result<()> {
        let mut result: Vec<BasicDropletInfo> = Vec::new();
        let mut fetch_next = true;
        let mut page = 1u64;
        let per_page: u64 = 100u64;
        while fetch_next {
            let loaded = self.client.list_droplets(per_page, page).await?;
            fetch_next = loaded.links.pages.next.is_some();
            result.extend(loaded.droplets.into_iter().map(BasicDropletInfo::from));
            page += 1;
        }
        self.save_droplets(result);
        Ok(())
    }

    fn record_droplets_metrics(&self) {
        let enabled_memory = self
            .configs
            .droplets
            .metrics
            .contains(&DropletMetricsTypes::Memory);
        let enabled_vcpu = self
            .configs
            .droplets
            .metrics
            .contains(&DropletMetricsTypes::VCpu);
        let enabled_disc = self
            .configs
            .droplets
            .metrics
            .contains(&DropletMetricsTypes::Disk);
        let enabled_status = self
            .configs
            .droplets
            .metrics
            .contains(&DropletMetricsTypes::Status);

        for droplet in self.store.read().iter() {
            let name = &droplet.name;
            if enabled_memory {
                self.metrics
                    .memory_gauge
                    .with(&std::collections::HashMap::<
                        &str,
                        &str,
                        std::hash::RandomState,
                    >::from([("droplet", name.as_ref())]))
                    .set(droplet.memory as f64);
            }

            if enabled_vcpu {
                self.metrics
                    .vcpu_gauge
                    .with(&std::collections::HashMap::<
                        &str,
                        &str,
                        std::hash::RandomState,
                    >::from([("droplet", name.as_ref())]))
                    .set(droplet.vcpus as f64);
            }

            if enabled_disc {
                self.metrics
                    .disk_gauge
                    .with(&std::collections::HashMap::<
                        &str,
                        &str,
                        std::hash::RandomState,
                    >::from([("droplet", name.as_ref())]))
                    .set(droplet.disk as f64);
            }

            if enabled_status {
                self.metrics
                    .status_gauge
                    .with(&std::collections::HashMap::<
                        &str,
                        &str,
                        std::hash::RandomState,
                    >::from([
                        ("droplet", name.as_ref()),
                        ("status", droplet.status.as_ref()),
                    ]))
                    .set(1_f64);
            }
        }
        let lock = self.store.read();
        let droplets: HashSet<_> = { lock.iter().map(|x| x.name.as_str()).collect() };

        // to prevent phantom droplets
        utils::remove_old_droplets(&self.metrics.memory_gauge, &droplets);
        utils::remove_old_droplets(&self.metrics.vcpu_gauge, &droplets);
        utils::remove_old_droplets(&self.metrics.disk_gauge, &droplets);
        utils::remove_old_droplets(&self.metrics.status_gauge, &droplets);
    }

    fn list_droplets(&self) -> Vec<BasicDropletInfo> {
        self.store.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::do_client::MockDigitalOceanClient;
    use crate::client::do_json_protocol::{DropletResponse, Links, ListDropletsResponse, Pages};
    use crate::config::config_model::AppSettings;
    use prometheus::core::Collector;
    use std::time::Duration as StdDuration;

    fn create_test_config() -> &'static AppSettings {
        let config = AppSettings {
            default_keys: vec!["test-key".to_string()],
            droplets: crate::config::config_model::DropletSettings {
                keys: vec![],
                url: "http://test.com/droplets".to_string(),
                interval: StdDuration::from_secs(60),
                metrics: vec![
                    crate::config::config_model::DropletMetricsTypes::Memory,
                    crate::config::config_model::DropletMetricsTypes::VCpu,
                    crate::config::config_model::DropletMetricsTypes::Disk,
                    crate::config::config_model::DropletMetricsTypes::Status,
                ],
            },
            apps: crate::config::config_model::AppPlatformSettings {
                keys: vec![],
                url: "http://test.com/apps".to_string(),
                interval: StdDuration::from_secs(60),
                metrics: vec![],
            },
            droplet_metrics: crate::config::config_model::DropletMetricsConfig {
                base_url: "http://test.com/metrics".to_string(),
                bandwidth: None,
                cpu: None,
                filesystem: None,
                memory: None,
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
    async fn test_load_droplets_single_page() {
        let mut mock_client = MockDigitalOceanClient::new();

        mock_client
            .expect_list_droplets()
            .withf(|per_page, page| *per_page == 100 && *page == 1)
            .times(1)
            .returning(|_, _| {
                Ok(ListDropletsResponse {
                    droplets: vec![
                        DropletResponse {
                            id: 123,
                            name: "test-droplet-1".to_string(),
                            memory: 1024,
                            vcpus: 1,
                            disk: 25,
                            locked: false,
                            status: "active".to_string(),
                        },
                        DropletResponse {
                            id: 456,
                            name: "test-droplet-2".to_string(),
                            memory: 2048,
                            vcpus: 2,
                            disk: 50,
                            locked: false,
                            status: "active".to_string(),
                        },
                    ],
                    links: Links {
                        pages: Pages {
                            next: None,
                            prev: None,
                            first: None,
                            last: None,
                        },
                    },
                })
            });

        let config = create_test_config();
        let registry = prometheus::Registry::new();
        let store = DropletStoreImpl::new(Arc::new(mock_client), config, registry).unwrap();

        let result = store.load_droplets().await;
        assert!(result.is_ok());

        let droplets = store.list_droplets();
        assert_eq!(droplets.len(), 2);
        assert_eq!(droplets[0].name, "test-droplet-1");
        assert_eq!(droplets[0].id, 123);
        assert_eq!(droplets[1].name, "test-droplet-2");
        assert_eq!(droplets[1].id, 456);
    }

    #[tokio::test]
    async fn test_load_droplets_multiple_pages() {
        let mut mock_client = MockDigitalOceanClient::new();

        // First page
        mock_client
            .expect_list_droplets()
            .withf(|per_page, page| *per_page == 100 && *page == 1)
            .times(1)
            .returning(|_, _| {
                Ok(ListDropletsResponse {
                    droplets: vec![DropletResponse {
                        id: 123,
                        name: "test-droplet-1".to_string(),
                        memory: 1024,
                        vcpus: 1,
                        disk: 25,
                        locked: false,
                        status: "active".to_string(),
                    }],
                    links: Links {
                        pages: Pages {
                            next: Some("http://next".to_string()),
                            prev: None,
                            first: None,
                            last: None,
                        },
                    },
                })
            });

        // Second page
        mock_client
            .expect_list_droplets()
            .withf(|per_page, page| *per_page == 100 && *page == 2)
            .times(1)
            .returning(|_, _| {
                Ok(ListDropletsResponse {
                    droplets: vec![DropletResponse {
                        id: 456,
                        name: "test-droplet-2".to_string(),
                        memory: 2048,
                        vcpus: 2,
                        disk: 50,
                        locked: false,
                        status: "active".to_string(),
                    }],
                    links: Links {
                        pages: Pages {
                            next: None,
                            prev: None,
                            first: None,
                            last: None,
                        },
                    },
                })
            });

        let config = create_test_config();
        let registry = prometheus::Registry::new();
        let store = DropletStoreImpl::new(Arc::new(mock_client), config, registry).unwrap();

        let result = store.load_droplets().await;
        assert!(result.is_ok());

        let droplets = store.list_droplets();
        assert_eq!(droplets.len(), 2);
        assert_eq!(droplets[0].id, 123);
        assert_eq!(droplets[1].id, 456);
    }

    #[tokio::test]
    async fn test_record_droplets_metrics() {
        let mock_client = MockDigitalOceanClient::new();
        let config = create_test_config();
        let registry = prometheus::Registry::new();
        let store = DropletStoreImpl::new(Arc::new(mock_client), config, registry).unwrap();

        // Manually populate store for testing metrics recording
        let droplets = vec![
            BasicDropletInfo {
                id: 123,
                name: "test-droplet".to_string(),
                memory: 1024,
                vcpus: 2,
                disk: 50,
                locked: false,
                status: "active".to_string(),
            },
            BasicDropletInfo {
                id: 456,
                name: "test-droplet-2".to_string(),
                memory: 2048,
                vcpus: 4,
                disk: 100,
                locked: true,
                status: "off".to_string(),
            },
        ];
        store.save_droplets(droplets);

        // Record metrics
        store.record_droplets_metrics();

        // Verify metrics were recorded
        let metrics = store.metrics.memory_gauge.collect();
        assert!(!metrics.is_empty());

        let droplets = store.list_droplets();
        assert_eq!(droplets.len(), 2);
        assert_eq!(droplets[0].memory, 1024);
        assert_eq!(droplets[1].memory, 2048);
    }

    #[tokio::test]
    async fn test_list_droplets_empty() {
        let mock_client = MockDigitalOceanClient::new();
        let config = create_test_config();
        let registry = prometheus::Registry::new();
        let store = DropletStoreImpl::new(Arc::new(mock_client), config, registry).unwrap();

        let droplets = store.list_droplets();
        assert_eq!(droplets.len(), 0);
    }
}
