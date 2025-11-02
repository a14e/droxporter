use crate::client::do_client::DigitalOceanClient;
use crate::client::do_json_protocol::{AppDataResponse, AppMetricMetaInfo, AppMetricsResponse};
use crate::config::config_model::AppSettings;
use crate::metrics::app_store::AppStore;
use crate::metrics::utils;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use prometheus::Opts;
use std::sync::Arc;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait AppMetricsService: Send + Sync {
    async fn load_cpu_percentage(&self) -> anyhow::Result<()>;
    async fn load_memory_percentage(&self) -> anyhow::Result<()>;
    async fn load_restart_count(
        &self,
        interval_start: DateTime<Utc>,
        interval_end: DateTime<Utc>,
    ) -> anyhow::Result<()>;
}

#[derive(Clone)]
pub struct AppMetricsServiceImpl {
    client: Arc<dyn DigitalOceanClient>,
    app_store: Arc<dyn AppStore>,
    metrics: LoaderAppMetrics,
}

impl AppMetricsServiceImpl {
    pub fn new(
        client: Arc<dyn DigitalOceanClient>,
        app_store: Arc<dyn AppStore>,
        _configs: &'static AppSettings,
        registry: prometheus::Registry,
    ) -> anyhow::Result<Self> {
        let result = Self {
            client,
            app_store,
            metrics: LoaderAppMetrics::new(registry)?,
        };
        Ok(result)
    }
}

#[derive(Clone)]
struct LoaderAppMetrics {
    app_cpu_percentage: prometheus::GaugeVec,
    app_memory_percentage: prometheus::GaugeVec,
    app_restart_count: prometheus::CounterVec,
}

impl LoaderAppMetrics {
    fn new(registry: prometheus::Registry) -> anyhow::Result<Self> {
        let app_cpu_percentage = prometheus::GaugeVec::new(
            Opts::new("droxporter_app_cpu_percentage", "App CPU %"),
            &["app", "app_component", "app_component_instance"],
        )?;
        let app_memory_percentage = prometheus::GaugeVec::new(
            Opts::new("droxporter_app_memory_percentage", "App Memory %"),
            &["app", "app_component", "app_component_instance"],
        )?;
        let app_restart_count = prometheus::CounterVec::new(
            Opts::new("droxporter_app_restart_count", "App restart count"),
            &["app", "app_component", "app_component_instance"],
        )?;
        registry.register(Box::new(app_cpu_percentage.clone()))?;
        registry.register(Box::new(app_memory_percentage.clone()))?;
        registry.register(Box::new(app_restart_count.clone()))?;
        let result = Self {
            app_cpu_percentage,
            app_memory_percentage,
            app_restart_count,
        };
        Ok(result)
    }
}

fn extract_app_meta_with_last_values(response: AppDataResponse) -> Vec<(AppMetricMetaInfo, f64)> {
    response
        .data
        .result
        .into_iter()
        .map(|x| {
            let last_point = last_point_for_app_metrics(&x);
            let info = x.metric;
            (info, last_point)
        })
        .collect()
}

fn extract_app_meta_with_sum_of_values(response: AppDataResponse) -> Vec<(AppMetricMetaInfo, f64)> {
    response
        .data
        .result
        .into_iter()
        .map(|x| {
            let sum = sum_for_app_metrics(&x);
            let info = x.metric;
            (info, sum)
        })
        .collect()
}

fn last_point_for_app_metrics(metrics: &AppMetricsResponse) -> f64 {
    metrics
        .values
        .iter()
        .max_by_key(|x| x.timestamp)
        .and_then(|x| x.value.parse::<f64>().ok())
        .unwrap_or(0f64)
}

fn sum_for_app_metrics(metrics: &AppMetricsResponse) -> f64 {
    metrics
        .values
        .iter()
        .map(|x| x.value.parse::<f64>().unwrap_or(0f64))
        .sum()
}

fn metrics_read_interval() -> Duration {
    // It seems that DO has a 10..15 second interval between points, so I think an interval of 1 minute is reasonable.
    Duration::minutes(1)
}

// a lot of boilerplate. but I don't think it would be changing too often
#[async_trait]
impl AppMetricsService for AppMetricsServiceImpl {
    async fn load_cpu_percentage(&self) -> anyhow::Result<()> {
        let interval_end = Utc::now();
        let interval_start = interval_end - metrics_read_interval();

        for app in self.app_store.list_apps().iter() {
            let res = self
                .client
                .get_app_cpu_percentage(app.id.clone(), interval_start, interval_end)
                .await?;
            for (meta, value) in extract_app_meta_with_last_values(res) {
                self.metrics
                    .app_cpu_percentage
                    .with_label_values(&[
                        app.name.as_str(),
                        meta.app_component.as_str(),
                        meta.app_component_instance.as_str(),
                    ])
                    .set(value);
            }
        }

        let apps = self.app_store.list_apps();
        let apps_names: ahash::HashSet<_> = apps.iter().map(|x| x.name.as_str()).collect();
        utils::remove_old_apps_for_gauge_metric(&self.metrics.app_cpu_percentage, &apps_names);

        Ok(())
    }

    async fn load_memory_percentage(&self) -> anyhow::Result<()> {
        let interval_end = Utc::now();
        let interval_start = interval_end - metrics_read_interval();

        for app in self.app_store.list_apps().iter() {
            let res = self
                .client
                .get_app_memory_percentage(app.id.clone(), interval_start, interval_end)
                .await?;
            for (meta, value) in extract_app_meta_with_last_values(res) {
                self.metrics
                    .app_memory_percentage
                    .with_label_values(&[
                        app.name.as_str(),
                        meta.app_component.as_str(),
                        meta.app_component_instance.as_str(),
                    ])
                    .set(value);
            }
        }

        let apps = self.app_store.list_apps();
        let apps_names: ahash::HashSet<_> = apps.iter().map(|x| x.name.as_str()).collect();
        utils::remove_old_apps_for_gauge_metric(&self.metrics.app_memory_percentage, &apps_names);

        Ok(())
    }

    async fn load_restart_count(
        &self,
        interval_start: DateTime<Utc>,
        interval_end: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        for app in self.app_store.list_apps().iter() {
            let res = self
                .client
                .get_app_restart_count(app.id.clone(), interval_start, interval_end)
                .await?;
            for (meta, value) in extract_app_meta_with_sum_of_values(res) {
                self.metrics
                    .app_restart_count
                    .with_label_values(&[
                        app.name.as_str(),
                        meta.app_component.as_str(),
                        meta.app_component_instance.as_str(),
                    ])
                    .inc_by(value);
            }
        }

        let apps = self.app_store.list_apps();
        let apps_names: ahash::HashSet<_> = apps.iter().map(|x| x.name.as_str()).collect();
        utils::remove_old_apps_for_counter_metric(&self.metrics.app_restart_count, &apps_names);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::do_client::MockDigitalOceanClient;
    use crate::client::do_json_protocol::{
        AppDataResponse, AppDataResult, AppMetricMetaInfo, AppMetricsResponse, MetricPoint,
    };
    use crate::config::config_model::AppSettings;
    use crate::metrics::app_store::{BasicAppInfo, MockAppStore};
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
                bandwidth: None,
                cpu: None,
                filesystem: None,
                memory: None,
                load: None,
            },
            app_metrics: crate::config::config_model::AppMetricsConfig {
                base_url: "http://test.com/app_metrics".to_string(),
                cpu_percentage: Some(crate::config::config_model::AppCpuPercentageSettings {
                    enabled: true,
                    interval: StdDuration::from_secs(60),
                    keys: vec![],
                }),
                memory_percentage: Some(crate::config::config_model::AppMemoryPercentageSettings {
                    enabled: true,
                    interval: StdDuration::from_secs(60),
                    keys: vec![],
                }),
                restart_count: Some(crate::config::config_model::AppRestartCountSettings {
                    enabled: true,
                    interval: StdDuration::from_secs(60),
                    keys: vec![],
                }),
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
    async fn test_load_cpu_percentage_success() {
        let mut mock_client = MockDigitalOceanClient::new();
        let mut mock_store = MockAppStore::new();

        let apps = vec![BasicAppInfo {
            id: "app-123".to_string(),
            name: "test-app".to_string(),
            active_deployment_phase: "ACTIVE".to_string(),
        }];

        mock_store
            .expect_list_apps()
            .times(2)
            .returning(move || apps.clone());

        mock_client
            .expect_get_app_cpu_percentage()
            .withf(|id, _start, _end| id == "app-123")
            .times(1)
            .returning(|_, _, _| {
                Ok(AppDataResponse {
                    status: "success".to_string(),
                    data: AppDataResult {
                        result: vec![AppMetricsResponse {
                            metric: AppMetricMetaInfo {
                                app_component: "web".to_string(),
                                app_component_instance: "web-0".to_string(),
                                app_owner_id: Some("owner-123".to_string()),
                                app_uuid: "app-123".to_string(),
                            },
                            values: vec![MetricPoint {
                                timestamp: 1682246520,
                                value: "45.5".to_string(),
                            }],
                        }],
                    },
                })
            });

        let config = create_test_config();
        let registry = prometheus::Registry::new();

        let service = AppMetricsServiceImpl::new(
            Arc::new(mock_client),
            Arc::new(mock_store),
            config,
            registry,
        )
        .unwrap();

        let result = service.load_cpu_percentage().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_load_memory_percentage_success() {
        let mut mock_client = MockDigitalOceanClient::new();
        let mut mock_store = MockAppStore::new();

        let apps = vec![BasicAppInfo {
            id: "app-456".to_string(),
            name: "test-app-2".to_string(),
            active_deployment_phase: "ACTIVE".to_string(),
        }];

        mock_store
            .expect_list_apps()
            .times(2)
            .returning(move || apps.clone());

        mock_client
            .expect_get_app_memory_percentage()
            .withf(|id, _start, _end| id == "app-456")
            .times(1)
            .returning(|_, _, _| {
                Ok(AppDataResponse {
                    status: "success".to_string(),
                    data: AppDataResult {
                        result: vec![AppMetricsResponse {
                            metric: AppMetricMetaInfo {
                                app_component: "worker".to_string(),
                                app_component_instance: "worker-0".to_string(),
                                app_owner_id: Some("owner-456".to_string()),
                                app_uuid: "app-456".to_string(),
                            },
                            values: vec![MetricPoint {
                                timestamp: 1682246520,
                                value: "67.8".to_string(),
                            }],
                        }],
                    },
                })
            });

        let config = create_test_config();
        let registry = prometheus::Registry::new();

        let service = AppMetricsServiceImpl::new(
            Arc::new(mock_client),
            Arc::new(mock_store),
            config,
            registry,
        )
        .unwrap();

        let result = service.load_memory_percentage().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_load_restart_count_success() {
        let mut mock_client = MockDigitalOceanClient::new();
        let mut mock_store = MockAppStore::new();

        let apps = vec![BasicAppInfo {
            id: "app-789".to_string(),
            name: "test-app-3".to_string(),
            active_deployment_phase: "ACTIVE".to_string(),
        }];

        mock_store
            .expect_list_apps()
            .times(2)
            .returning(move || apps.clone());

        mock_client
            .expect_get_app_restart_count()
            .withf(|id, _start, _end| id == "app-789")
            .times(1)
            .returning(|_, _, _| {
                Ok(AppDataResponse {
                    status: "success".to_string(),
                    data: AppDataResult {
                        result: vec![AppMetricsResponse {
                            metric: AppMetricMetaInfo {
                                app_component: "api".to_string(),
                                app_component_instance: "api-0".to_string(),
                                app_owner_id: Some("owner-789".to_string()),
                                app_uuid: "app-789".to_string(),
                            },
                            values: vec![
                                MetricPoint {
                                    timestamp: 1682246520,
                                    value: "2".to_string(),
                                },
                                MetricPoint {
                                    timestamp: 1682246580,
                                    value: "1".to_string(),
                                },
                            ],
                        }],
                    },
                })
            });

        let config = create_test_config();
        let registry = prometheus::Registry::new();

        let service = AppMetricsServiceImpl::new(
            Arc::new(mock_client),
            Arc::new(mock_store),
            config,
            registry,
        )
        .unwrap();

        let start = Utc::now() - Duration::minutes(5);
        let end = Utc::now();

        let result = service.load_restart_count(start, end).await;
        assert!(result.is_ok());
    }
}
