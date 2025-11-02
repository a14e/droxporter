use crate::client::do_client::DigitalOceanClient;
use crate::client::do_json_protocol::AppResponse;
use crate::config::config_model::{AppMetricsTypes, AppSettings};
use crate::metrics::utils;
use ahash::HashSet;
use async_trait::async_trait;
use parking_lot::RwLock;
use prometheus::Opts;
use std::sync::Arc;

#[cfg_attr(test, mockall::automock)]
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
    pub fn new(
        client: Arc<dyn DigitalOceanClient>,
        configs: &'static AppSettings,
        registry: prometheus::Registry,
    ) -> anyhow::Result<Self> {
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
            Opts::new(
                "droxporter_app_active_deployment_phase",
                "The label active_deployment_phase indicates the current phase for the app. Values is always 1.",
            ),
            &["app", "active_deployment_phase"],
        )?;

        registry.register(Box::new(active_gauge.clone()))?;

        let result = Self { active_gauge };
        Ok(result)
    }
}

impl AppStoreImpl {
    fn save_apps(&self, apps: Vec<BasicAppInfo>) {
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
            fetch_next = loaded.links.pages.next.is_some();
            result.extend(loaded.apps.into_iter().map(BasicAppInfo::from));
            page += 1;
        }
        self.save_apps(result);
        Ok(())
    }

    fn record_app_metrics(&self) {
        let enabled_active_deployment_phase = self
            .configs
            .apps
            .metrics
            .contains(&AppMetricsTypes::ActiveDeploymentPhase);

        for app in self.store.read().iter() {
            if enabled_active_deployment_phase {
                self.metrics
                    .active_gauge
                    .with(&std::collections::HashMap::<
                        &str,
                        &str,
                        std::hash::RandomState,
                    >::from([
                        ("app", app.name.as_ref()),
                        (
                            "active_deployment_phase",
                            app.active_deployment_phase.as_ref(),
                        ),
                    ]))
                    .set(1_f64);
            }
        }

        let lock = self.store.read();
        let apps: HashSet<_> = { lock.iter().map(|x| x.name.as_str()).collect() };

        // to prevent phantom apps
        utils::remove_old_apps_for_gauge_metric(&self.metrics.active_gauge, &apps);
    }

    fn list_apps(&self) -> Vec<BasicAppInfo> {
        self.store.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::do_client::MockDigitalOceanClient;
    use crate::client::do_json_protocol::{
        AppActiveDeployment, AppResponse, AppSpec, Links, ListAppsResponse, Pages,
    };
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
                metrics: vec![],
            },
            apps: crate::config::config_model::AppPlatformSettings {
                keys: vec![],
                url: "http://test.com/apps".to_string(),
                interval: StdDuration::from_secs(60),
                metrics: vec![crate::config::config_model::AppMetricsTypes::ActiveDeploymentPhase],
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
    async fn test_load_apps_single_page() {
        let mut mock_client = MockDigitalOceanClient::new();

        mock_client
            .expect_list_apps()
            .withf(|per_page, page| *per_page == 100 && *page == 1)
            .times(1)
            .returning(|_, _| {
                Ok(ListAppsResponse {
                    apps: vec![
                        AppResponse {
                            id: "app-123".to_string(),
                            spec: AppSpec {
                                name: "test-app-1".to_string(),
                            },
                            active_deployment: Some(AppActiveDeployment {
                                id: "dep-123".to_string(),
                                cause: "manual".to_string(),
                                phase: "ACTIVE".to_string(),
                            }),
                        },
                        AppResponse {
                            id: "app-456".to_string(),
                            spec: AppSpec {
                                name: "test-app-2".to_string(),
                            },
                            active_deployment: Some(AppActiveDeployment {
                                id: "dep-456".to_string(),
                                cause: "manual".to_string(),
                                phase: "DEPLOYING".to_string(),
                            }),
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
        let store = AppStoreImpl::new(Arc::new(mock_client), config, registry).unwrap();

        let result = store.load_apps().await;
        assert!(result.is_ok());

        let apps = store.list_apps();
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "test-app-1");
        assert_eq!(apps[0].id, "app-123");
        assert_eq!(apps[0].active_deployment_phase, "ACTIVE");
        assert_eq!(apps[1].name, "test-app-2");
        assert_eq!(apps[1].id, "app-456");
        assert_eq!(apps[1].active_deployment_phase, "DEPLOYING");
    }

    #[tokio::test]
    async fn test_load_apps_multiple_pages() {
        let mut mock_client = MockDigitalOceanClient::new();

        // First page
        mock_client
            .expect_list_apps()
            .withf(|per_page, page| *per_page == 100 && *page == 1)
            .times(1)
            .returning(|_, _| {
                Ok(ListAppsResponse {
                    apps: vec![AppResponse {
                        id: "app-123".to_string(),
                        spec: AppSpec {
                            name: "test-app-1".to_string(),
                        },
                        active_deployment: Some(AppActiveDeployment {
                            id: "dep-123".to_string(),
                            cause: "manual".to_string(),
                            phase: "ACTIVE".to_string(),
                        }),
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
            .expect_list_apps()
            .withf(|per_page, page| *per_page == 100 && *page == 2)
            .times(1)
            .returning(|_, _| {
                Ok(ListAppsResponse {
                    apps: vec![AppResponse {
                        id: "app-456".to_string(),
                        spec: AppSpec {
                            name: "test-app-2".to_string(),
                        },
                        active_deployment: None,
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
        let store = AppStoreImpl::new(Arc::new(mock_client), config, registry).unwrap();

        let result = store.load_apps().await;
        assert!(result.is_ok());

        let apps = store.list_apps();
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].id, "app-123");
        assert_eq!(apps[0].active_deployment_phase, "ACTIVE");
        assert_eq!(apps[1].id, "app-456");
        assert_eq!(apps[1].active_deployment_phase, "UNKNOWN");
    }

    #[tokio::test]
    async fn test_record_app_metrics() {
        let mock_client = MockDigitalOceanClient::new();
        let config = create_test_config();
        let registry = prometheus::Registry::new();
        let store = AppStoreImpl::new(Arc::new(mock_client), config, registry).unwrap();

        // Manually populate store for testing metrics recording
        let apps = vec![
            BasicAppInfo {
                id: "app-123".to_string(),
                name: "test-app".to_string(),
                active_deployment_phase: "ACTIVE".to_string(),
            },
            BasicAppInfo {
                id: "app-456".to_string(),
                name: "test-app-2".to_string(),
                active_deployment_phase: "SUPERSEDED".to_string(),
            },
        ];
        store.save_apps(apps);

        // Record metrics
        store.record_app_metrics();

        // Verify metrics were recorded
        let metrics = store.metrics.active_gauge.collect();
        assert!(!metrics.is_empty());

        let apps = store.list_apps();
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].active_deployment_phase, "ACTIVE");
        assert_eq!(apps[1].active_deployment_phase, "SUPERSEDED");
    }

    #[tokio::test]
    async fn test_list_apps_empty() {
        let mock_client = MockDigitalOceanClient::new();
        let config = create_test_config();
        let registry = prometheus::Registry::new();
        let store = AppStoreImpl::new(Arc::new(mock_client), config, registry).unwrap();

        let apps = store.list_apps();
        assert_eq!(apps.len(), 0);
    }
}
