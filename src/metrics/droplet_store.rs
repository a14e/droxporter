use std::sync::Arc;
use ahash::HashMap;
use async_trait::async_trait;
use parking_lot::{RawRwLock, RwLock};
use parking_lot::lock_api::ArcRwLockReadGuard;
use crate::client::do_client::DigitalOceanClient;
use crate::client::do_json_protocol::DropletResponse;
use crate::config::config_model::{AppSettings, DropletMetricsTypes};

#[async_trait]
pub trait DropletStore {
    async fn load_droplets(&self) -> anyhow::Result<()>;

    fn record_droplets_metrics(&self);

    fn list_droplets(&self) -> Vec<BasicDropletInfo>;
}

#[derive(Clone)]
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



pub struct DropletStoreImpl<DOClient> {
    store: Arc<RwLock<Vec<BasicDropletInfo>>>,
    client: DOClient,
    configs: &'static AppSettings,
    metrics: DropletsMetrics
}


#[derive(Clone)]
struct DropletsMetrics {
    memory_gauge: prometheus::GaugeVec,
    vcpu_gauge: prometheus::GaugeVec,
    disk_gauge: prometheus::GaugeVec,
    status_gauge: prometheus::GaugeVec,
}


impl<DOClient: DigitalOceanClient + Clone> DropletStoreImpl<DOClient> {
    fn safe_droplets(&self,
                     droplets: Vec<BasicDropletInfo>) {
        *self.store.write() = droplets;
    }
}

#[async_trait]
impl<DOClient: DigitalOceanClient + Clone + Send + Sync> DropletStore for DropletStoreImpl<DOClient> {
    async fn load_droplets(&self) -> anyhow::Result<()> {
        let mut result: Vec<BasicDropletInfo> = Vec::new();
        let mut fetch_next = true;
        let mut page = 1u64;
        let per_page: u64 = 100u64;
        while fetch_next {
            let loaded = self.client.list_droplets(per_page, page).await?;
            fetch_next = !loaded.droplets.is_empty();
            result.extend(loaded.droplets.into_iter().map(BasicDropletInfo::from));
            page += 1;
        }
        self.safe_droplets(result);
        Ok(())
    }

    fn record_droplets_metrics(&self)  {
        let enabled_memory = self.configs.droplets.metrics.contains(&DropletMetricsTypes::Memory);
        let enabled_vcpu = self.configs.droplets.metrics.contains(&DropletMetricsTypes::VCpu);
        let enabled_disc = self.configs.droplets.metrics.contains(&DropletMetricsTypes::Disk);
        let enabled_status = self.configs.droplets.metrics.contains(&DropletMetricsTypes::Status);

        // to prevent phantom droplets
        self.metrics.memory_gauge.reset();
        self.metrics.vcpu_gauge.reset();
        self.metrics.disk_gauge.reset();
        self.metrics.status_gauge.reset();


        for droplet in self.store.read().iter() {
            let name = &droplet.name;
            if enabled_memory {
                self.metrics.memory_gauge
                    .with(&std::collections::HashMap::from([
                        ("droplet", name.as_ref())
                    ])).set(droplet.memory as f64);
            }

            if enabled_vcpu {
                self.metrics.vcpu_gauge
                    .with(&std::collections::HashMap::from([
                        ("droplet", name.as_ref())
                    ])).set(droplet.vcpus as f64);
            }

            if enabled_disc {
                self.metrics.disk_gauge
                    .with(&std::collections::HashMap::from([
                        ("droplet", name.as_ref())
                    ])).set(droplet.disk as f64);
            }

            if enabled_status {
                self.metrics.status_gauge
                    .with(&std::collections::HashMap::from([
                        ("droplet", name.as_ref()),
                        ("status", droplet.status.as_ref()),
                    ])).set(1 as f64);
            }
        }

    }

    fn list_droplets(&self) -> Vec<BasicDropletInfo> {
        self.store.read().clone()
    }
}


