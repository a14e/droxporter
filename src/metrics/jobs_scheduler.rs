use std::time::Duration;
use async_trait::async_trait;
use tracing::{error, info};
use crate::client::do_client::DigitalOceanClient;
use crate::config::config_model::AppSettings;
use crate::metrics::droplet_merics_loader::DropletMetricsService;
use crate::metrics::droplet_store::DropletStore;

#[async_trait]
pub trait MetricsScheduler {
    async fn run_droplets_loading(&self) -> anyhow::Result<()>;
    async fn run_bandwidth_metrics_loading(&self) -> anyhow::Result<()>;
    async fn run_cpu_metrics_loading(&self) -> anyhow::Result<()>;
    async fn run_filesystem_metrics_loading(&self) -> anyhow::Result<()>;
    async fn run_memory_metrics_loading(&self) -> anyhow::Result<()>;
    async fn run_load_metrics_loading(&self) -> anyhow::Result<()>;
}


struct MetricsSchedulerImpl<DoClient, DropletStore, DropletMetricsService> {
    client: DoClient,
    configs: &'static AppSettings,
    droplet_store: DropletStore,
    metrics_service: DropletMetricsService
}

// There are a lot of duplications here. but it's much simpler for me to debug this way
// If you know a better and more readable approach, please submit a merge request =)
#[async_trait]
impl<Client, Store, Metrics> MetricsScheduler for MetricsSchedulerImpl<Client, Store, Metrics>
    where Client: DigitalOceanClient + Clone + Send + Sync,
          Store: DropletStore + Clone + Send + Sync,
          Metrics: DropletMetricsService + Clone + Send + Sync, {
    async fn run_droplets_loading(&self) -> anyhow::Result<()> {
        info!("Starting droplets loading loop");


        let mut first = true;
        loop {
            if !first {
                tokio::time::sleep(self.configs.droplets.interval).await;
            }
            first = false;

            if let Err(e) = self.droplet_store.load_droplets().await {
                error!("Droplets loading failed with err {e}");
                continue;
            }
            self.droplet_store.record_droplets_metrics();
        }

        Ok(())
    }

    async fn run_bandwidth_metrics_loading(&self) -> anyhow::Result<()> {
        if let Some(bandwidth) = self.configs.metrics.bandwidth.as_ref() {
            if !bandwidth.enabled {
                info!("Bandwidth metrics are disabled");
                return Ok(());
            }
            info!("Staring bandwidth metrics loading loop");

            // timeout for initial load
            // looks ugly, but simple =)
            let first_delay = Duration::from_secs(10).min(bandwidth.interval);
            let mut first = true;
            loop {
                let timeout = if first { first_delay } else { bandwidth.interval };
                first = false;
                tokio::time::sleep(timeout).await;

                if let Err(e) = self.metrics_service.load_bandwidth().await {
                    error!("Bandwidth metrics loading failed with err {e}");
                    continue;
                }
            }
        }
        Ok(())
    }

    async fn run_cpu_metrics_loading(&self) -> anyhow::Result<()> {
        if let Some(cpu) = self.configs.metrics.cpu.as_ref() {
            if !cpu.enabled {
                info!("Cpu metrics are disabled");
                return Ok(());
            }
            info!("Staring cpu metrics loading loop");

            // timeout for initial load
            // looks ugly, but simple =)
            let first_delay = Duration::from_secs(10).min(cpu.interval);
            let mut first = true;
            loop {
                let timeout = if first { first_delay } else { cpu.interval };
                first = false;
                tokio::time::sleep(timeout).await;

                if let Err(e) = self.metrics_service.load_cpu_metrics().await {
                    error!("Cpu metrics loading failed with err {e}");
                    continue;
                }
            }
        }
        Ok(())
    }

    async fn run_filesystem_metrics_loading(&self) -> anyhow::Result<()> {
        if let Some(filesystem) = self.configs.metrics.filesystem.as_ref() {
            if !filesystem.enabled {
                info!("Filesystem metrics are disabled");
                return Ok(());
            }
            info!("Staring filesystem metrics loading loop");

            // timeout for initial load
            // looks ugly, but simple =)
            let first_delay = Duration::from_secs(10).min(filesystem.interval);
            let mut first = true;
            loop {
                let timeout = if first { first_delay } else { filesystem.interval };
                first = false;
                tokio::time::sleep(timeout).await;

                if let Err(e) = self.metrics_service.load_filesystem_metrics().await {
                    error!("Filesystem metrics loading failed with err {e}");
                    continue;
                }
            }
        }
        Ok(())
    }

    async fn run_memory_metrics_loading(&self) -> anyhow::Result<()> {
        if let Some(memory) = self.configs.metrics.memory.as_ref() {
            if !memory.enabled {
                info!("Memory metrics are disabled");
                return Ok(());
            }
            info!("Staring memory metrics loading loop");

            // timeout for initial load
            // looks ugly, but simple =)
            let first_delay = Duration::from_secs(10).min(memory.interval);
            let mut first = true;
            loop {
                let timeout = if first { first_delay } else { memory.interval };
                first = false;
                tokio::time::sleep(timeout).await;

                if let Err(e) = self.metrics_service.load_memory_metrics().await {
                    error!("Memory metrics loading failed with err {e}");
                    continue;
                }
            }
        }
        Ok(())
    }

    async fn run_load_metrics_loading(&self) -> anyhow::Result<()> {
        if let Some(load) = self.configs.metrics.load.as_ref() {
            if !load.enabled {
                info!("Load metrics are disabled");
                return Ok(());
            }
            info!("Staring load metrics loading loop");


            // timeout for initial load
            // looks ugly, but simple =)
            let first_delay = Duration::from_secs(10).min(load.interval);
            let mut first = true;
            loop {
                let timeout = if first { first_delay } else { load.interval };
                first = false;
                tokio::time::sleep(timeout).await;

                // load load =(
                if let Err(e) = self.metrics_service.load_load_metrics().await {
                    error!("Load metrics loading failed with err {e}");
                    continue;
                }
            }
        }
        Ok(())
    }
}











