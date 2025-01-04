use std::sync::Arc;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use prometheus::Opts;
use crate::client::do_client::DigitalOceanClient;
use crate::client::do_json_protocol::{AppDataResponse, AppMetricMetaInfo, AppMetricsResponse};
use crate::config::config_model::AppSettings;
use crate::metrics::app_store::AppStore;
use crate::metrics::utils;

#[async_trait]
pub trait AppMetricsService: Send + Sync {
    async fn load_cpu_percentage(&self) -> anyhow::Result<()>;
    async fn load_memory_percentage(&self) -> anyhow::Result<()>;
    async fn load_restart_count(&self, interval_start: DateTime<Utc>, interval_end: DateTime<Utc>) -> anyhow::Result<()>;
}


#[derive(Clone)]
pub struct AppMetricsServiceImpl {
    client: Arc<dyn DigitalOceanClient>,
    app_store: Arc<dyn AppStore>,
    // There are no valid per metric settings yet.
    configs: &'static AppSettings,
    metrics: LoaderAppMetrics,
}

impl AppMetricsServiceImpl {
    pub fn new(client: Arc<dyn DigitalOceanClient>,
               app_store: Arc<dyn AppStore>,
               configs: &'static AppSettings,
               registry: prometheus::Registry) -> anyhow::Result<Self> {
        let result = Self {
            client,
            app_store,
            configs,
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
    response.data.result.into_iter()
        .map(|x| {
            let last_point = last_point_for_app_metrics(&x);
            let info = x.metric;
            (info, last_point)
        }).collect()
}

fn extract_app_meta_with_sum_of_values(response: AppDataResponse) -> Vec<(AppMetricMetaInfo, f64)> {
    response.data.result.into_iter()
        .map(|x| {
            let sum = sum_for_app_metrics(&x);
            let info = x.metric;
            (info, sum)
        }).collect()
}

fn last_point_for_app_metrics(metrics: &AppMetricsResponse) -> f64 {
    metrics.values.iter()
        .max_by_key(|x| x.timestamp)
        .and_then(|x| x.value.parse::<f64>().ok())
        .unwrap_or(0f64)
}

fn sum_for_app_metrics(metrics: &AppMetricsResponse) -> f64 {
    metrics.values.iter()
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
            let res = self.client
                .get_app_cpu_percentage(
                    app.id.clone(),
                    interval_start,
                    interval_end
                ).await?;
            for (meta, value) in extract_app_meta_with_last_values(res) {
                self.metrics.app_cpu_percentage
                .with_label_values(&[
                        &app.name.as_str(),
                        &meta.app_component.as_str(),
                        &meta.app_component_instance.as_str(),
                    ]).set(value);
            }
        }

        let apps = self.app_store.list_apps();
        let apps_names: ahash::HashSet<_> = apps
            .iter()
            .map(|x| x.name.as_str())
            .collect();
        utils::remove_old_apps_for_gauge_metric(&self.metrics.app_cpu_percentage, &apps_names);

        Ok(())
    }

    async fn load_memory_percentage(&self) -> anyhow::Result<()> {
        let interval_end = Utc::now();
        let interval_start = interval_end - metrics_read_interval();

        for app in self.app_store.list_apps().iter() {
            let res = self.client
                .get_app_memory_percentage(
                    app.id.clone(),
                    interval_start,
                    interval_end
                ).await?;
            for (meta, value) in extract_app_meta_with_last_values(res) {
                self.metrics.app_memory_percentage
                .with_label_values(&[
                        &app.name.as_str(),
                        &meta.app_component.as_str(),
                        &meta.app_component_instance.as_str(),
                    ]).set(value);
            }
        }

        let apps = self.app_store.list_apps();
        let apps_names: ahash::HashSet<_> = apps
            .iter()
            .map(|x| x.name.as_str())
            .collect();
        utils::remove_old_apps_for_gauge_metric(&self.metrics.app_memory_percentage, &apps_names);

        Ok(())
    }

    async fn load_restart_count(&self, interval_start: DateTime<Utc>, interval_end: DateTime<Utc>) -> anyhow::Result<()> {
        for app in self.app_store.list_apps().iter() {
            let res = self.client
                .get_app_restart_count(
                    app.id.clone(),
                    interval_start,
                    interval_end
                ).await?;
            for (meta, value) in extract_app_meta_with_sum_of_values(res) {
                self.metrics.app_restart_count
                .with_label_values(&[
                        &app.name.as_str(),
                        &meta.app_component.as_str(),
                        &meta.app_component_instance.as_str(),
                    ]).inc_by(value);
            }
        }

        let apps = self.app_store.list_apps();
        let apps_names: ahash::HashSet<_> = apps
            .iter()
            .map(|x| x.name.as_str())
            .collect();
        utils::remove_old_apps_for_counter_metric(&self.metrics.app_restart_count, &apps_names);

        Ok(())
    }
}
