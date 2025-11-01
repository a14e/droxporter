use crate::client::do_client::DigitalOceanClient;
use crate::client::do_json_protocol::DropletResponse;
use crate::config::config_model::{AppSettings, DropletMetricsTypes};
use crate::metrics::utils;
use ahash::HashSet;
use async_trait::async_trait;
use parking_lot::RwLock;
use prometheus::Opts;
use std::sync::Arc;

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
