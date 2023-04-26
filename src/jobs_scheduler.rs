use async_trait::async_trait;
use crate::client::do_client::DigitalOceanClient;
use crate::config::config_model::AppSettings;

#[async_trait]
pub trait MetricsScheduler {
    async fn run_droplets_loading() -> anyhow::Result<()>;
    async fn run_bandwidth_metrics_loading() -> anyhow::Result<()>;
    async fn run_cpu_metrics_loading() -> anyhow::Result<()>;
    async fn run_filesystem_metrics_loading() -> anyhow::Result<()>;
    async fn run_memory_metrics_loading() -> anyhow::Result<()>;
    async fn run_load_metrics_loading() -> anyhow::Result<()>;
}


struct MetricsSchedulerImpl<DoClinent: DigitalOceanClient> {
    client: DoClinent,
    configs: &'static AppSettings,
}














