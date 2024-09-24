use std::sync::Arc;
use ahash::HashSet;
use async_trait::async_trait;
use parking_lot::RwLock;
use prometheus::Opts;
use crate::client::do_client::DigitalOceanClient;
use crate::client::do_json_protocol::AppResponse;
use crate::config::config_model::{AppMetricsTypes, AppSettings};
use crate::metrics::utils;

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
    pub active_deployment_phase: String,
}

impl From<AppResponse> for BasicAppInfo {
    fn from(value: AppResponse) -> Self {
        Self {
            id: value.id,
            name: value.spec.name,
            active_deployment_phase: match value.active_deployment {
                Some(active_deployment) => active_deployment.phase,
                None => "UNKNOWN".to_string(),
            },
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
    active_gauge: prometheus::GaugeVec,
}

impl AppMetrics {
    fn new(registry: prometheus::Registry) -> anyhow::Result<Self> {
        let active_gauge = prometheus::GaugeVec::new(
            Opts::new("droxporter_app_active_deployment_phase", "The label active_deployment_phase indicates the current phase for the app. Values is always 1."),
            &["app", "active_deployment_phase"],
        )?;

        registry.register(Box::new(active_gauge.clone()))?;

        let result = Self {
            active_gauge,
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
        let enabled_active_deployment_phase = self.configs.apps.metrics.contains(&AppMetricsTypes::ActiveDeploymentPhase);

        for app in self.store.read().iter() {
            if enabled_active_deployment_phase {
                self.metrics.active_gauge
                    .with(&std::collections::HashMap::from([
                        ("app", app.name.as_ref()),
                        ("active_deployment_phase", app.active_deployment_phase.as_ref()),
                    ])).set(1 as f64);
            }
        }

        let lock = self.store.read();
        let apps: HashSet<_> = {
            lock.iter().map(|x| x.name.as_str()).collect()
        };

        // to prevent phantom apps
        utils::remove_old_apps_for_gauge_metric(&self.metrics.active_gauge, &apps);
    }

    fn list_apps(&self) -> Vec<BasicAppInfo> {
        self.store.read().clone()
    }
}


