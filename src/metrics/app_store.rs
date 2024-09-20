use std::sync::Arc;
use ahash::{HashSet};
use async_trait::async_trait;
use parking_lot::{RwLock};
use prometheus::Opts;
use crate::client::do_client::DigitalOceanClient;
use crate::client::do_json_protocol::AppResponse;
use crate::config::config_model::{AppSettings};

#[async_trait]
pub trait AppStore: Send + Sync {
    async fn load_apps(&self) -> anyhow::Result<()>;

    fn record_app_metrics(&self);

    fn list_apps(&self) -> Vec<BasicAppInfo>;
}

#[derive(Clone)]
pub struct BasicAppInfo {
    pub id: String,
    pub name: String,
}

impl From<AppResponse> for BasicAppInfo {
    fn from(value: AppResponse) -> Self {
        Self {
            id: value.id,
            name: value.spec.name,
        }
    }
}

#[derive(Clone)]
pub struct AppStoreImpl {
    store: Arc<RwLock<Vec<BasicAppInfo>>>,
    client: Arc<dyn DigitalOceanClient>,
    configs: &'static AppSettings,
    metrics: AppMetrics,
}

impl AppStoreImpl {
    pub fn new(client: Arc<dyn DigitalOceanClient>,
               configs: &'static AppSettings,
               registry: prometheus::Registry) -> anyhow::Result<Self> {
        let result = Self {
            store: Arc::new(RwLock::new(vec![])),
            client,
            configs,
            metrics: AppMetrics::new(registry)?,
        };
        Ok(result)
    }
}


#[derive(Clone)]
struct AppMetrics {
    up_gauge: prometheus::GaugeVec,
}

impl AppMetrics {
    fn new(registry: prometheus::Registry) -> anyhow::Result<Self> {
        let up_gauge = prometheus::GaugeVec::new(
            Opts::new("droxporter_app_up", "Constant of 1 (DEBUG)"),
            &["app"],
        )?;

        registry.register(Box::new(up_gauge.clone()))?;

        let result = Self {
            up_gauge,
        };
        Ok(result)
    }
}


impl AppStoreImpl {
    fn save_apps(&self,
                 apps: Vec<BasicAppInfo>) {
        *self.store.write() = apps;
    }
}


#[async_trait]
impl AppStore for AppStoreImpl {
    async fn load_apps(&self) -> anyhow::Result<()> {
        let mut result: Vec<BasicAppInfo> = Vec::new();
        let mut fetch_next = true;
        let mut page = 1u64;
        let per_page: u64 = 100u64;
        while fetch_next {
            let loaded = self.client.list_apps(per_page, page).await?;
            fetch_next = !loaded.apps.is_empty();
            result.extend(loaded.apps.into_iter().map(BasicAppInfo::from));
            page += 1;
        }
        self.save_apps(result);
        Ok(())
    }

    fn record_app_metrics(&self) {
        // let enabled_memory = self.configs.droplets.metrics.contains(&DropletMetricsTypes::Memory);
        // let enabled_vcpu = self.configs.droplets.metrics.contains(&DropletMetricsTypes::VCpu);
        // let enabled_disc = self.configs.droplets.metrics.contains(&DropletMetricsTypes::Disk);
        // let enabled_status = self.configs.droplets.metrics.contains(&DropletMetricsTypes::Status);

        for app in self.store.read().iter() {
            let name = &app.name;

            use std::collections::HashMap;
            
            let mut labels = HashMap::new();
            labels.insert("app", name.as_ref());
            
            self.metrics.up_gauge
                .with(&labels)
                .set(1.0 as f64);
            // if enabled_memory {
            //     self.metrics.memory_gauge
            //         .with(&std::collections::HashMap::from([
            //             ("droplet", name.as_ref())
            //         ])).set(app.memory as f64);
            // }

            // if enabled_vcpu {
            //     self.metrics.vcpu_gauge
            //         .with(&std::collections::HashMap::from([
            //             ("droplet", name.as_ref())
            //         ])).set(app.vcpus as f64);
            // }

            // if enabled_disc {
            //     self.metrics.disk_gauge
            //         .with(&std::collections::HashMap::from([
            //             ("droplet", name.as_ref())
            //         ])).set(app.disk as f64);
            // }

            // if enabled_status {
            //     self.metrics.status_gauge
            //         .with(&std::collections::HashMap::from([
            //             ("droplet", name.as_ref()),
            //             ("status", app.status.as_ref()),
            //         ])).set(1 as f64);
            // }
        }
        let lock = self.store.read();
        let apps: HashSet<_> = {
            lock.iter().map(|x| x.name.as_str()).collect()
        };

        // // to prevent phantom droplets
        // utils::remove_old_droplets(&self.metrics.memory_gauge, &droplets);
        // utils::remove_old_droplets(&self.metrics.vcpu_gauge, &droplets);
        // utils::remove_old_droplets(&self.metrics.disk_gauge, &droplets);
        // utils::remove_old_droplets(&self.metrics.status_gauge, &droplets);
    }

    fn list_apps(&self) -> Vec<BasicAppInfo> {
        self.store.read().clone()
    }
}


