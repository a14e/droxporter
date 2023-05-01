use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use prometheus::{HistogramOpts, Opts, Registry};
use tokio::time::Instant;
use tracing::{error, info};
use crate::config::config_model::{AgentMetricsType, AppSettings};
use crate::metrics::agent_metrics::AgentMetricsService;
use crate::metrics::droplet_metrics_loader::DropletMetricsService;
use crate::metrics::droplet_store::DropletStore;
use crate::metrics::utils::DROXPORTER_DEFAULT_BUCKETS;

#[async_trait]
pub trait MetricsScheduler: Send + Sync {
    async fn run_droplets_loading(&self) -> anyhow::Result<()>;
    async fn run_bandwidth_metrics_loading(&self) -> anyhow::Result<()>;
    async fn run_cpu_metrics_loading(&self) -> anyhow::Result<()>;
    async fn run_filesystem_metrics_loading(&self) -> anyhow::Result<()>;
    async fn run_memory_metrics_loading(&self) -> anyhow::Result<()>;
    async fn run_load_metrics_loading(&self) -> anyhow::Result<()>;
    async fn run_agent_metrics_loading(&self) -> anyhow::Result<()>;
}

pub struct MetricsSchedulerImpl {
    configs: &'static AppSettings,
    droplet_store: Arc<dyn DropletStore>,
    metrics_service: Arc<dyn DropletMetricsService>,
    agent_service: Arc<dyn AgentMetricsService>,

    jobs_counter: prometheus::CounterVec,
    jobs_histogram: prometheus::HistogramVec,
}


impl MetricsSchedulerImpl {
    pub fn new(configs: &'static AppSettings,
               droplet_store: Arc<dyn DropletStore>,
               metrics_service: Arc<dyn DropletMetricsService>,
               agent_service: Arc<dyn AgentMetricsService>,
               registry: Registry) -> anyhow::Result<Self> {
        let jobs_counter = prometheus::CounterVec::new(
            Opts::new("droxporter_jobs_counter", "Counter of droxporter jobs"),
            &["type", "result"],
        )?;
        let jobs_histogram = prometheus::HistogramVec::new(
            HistogramOpts::new("droxporter_jobs_time_histogram_seconds", "Time of droxporter jobs")
                .buckets((*DROXPORTER_DEFAULT_BUCKETS).into()),
            &["type", "result"],
        )?;
        registry.register(Box::new(jobs_counter.clone()))?;
        registry.register(Box::new(jobs_histogram.clone()))?;

        let result = Self {
            configs,
            droplet_store,
            metrics_service,
            agent_service,
            jobs_counter,
            jobs_histogram,
        };
        Ok(result)
    }


    fn are_metrics_enabled(&self) -> bool {
        self.configs.agent_metrics.enabled && {
            self.configs.agent_metrics.metrics.contains(&AgentMetricsType::Jobs)
        }
    }


    fn record_job_metrics(&self,
                          job_name: &str,
                          success: bool,
                          start_time: Instant) {
        if !self.are_metrics_enabled() {
            return;
        }

        let elasped_time_seconds = start_time.elapsed().as_millis() as f64 / 1000.0f64;
        let result = if success {
            "success"
        } else {
            "fail"
        };
        self.jobs_histogram
            .with_label_values(&[job_name, result])
            .observe(elasped_time_seconds);
        self.jobs_counter
            .with_label_values(&[job_name, result])
            .inc();
    }
}

// There are a lot of duplications here. but it's much simpler for me to debug this way
// If you know a better and more readable approach, please submit a merge request =)
#[async_trait]
impl MetricsScheduler for MetricsSchedulerImpl {
    async fn run_droplets_loading(&self) -> anyhow::Result<()> {
        info!("Starting droplets loading loop");


        let mut first = true;
        loop {
            if !first {
                tokio::time::sleep(self.configs.droplets.interval).await;
            }
            first = false;
            let start = Instant::now();

            if let Err(e) = self.droplet_store.load_droplets().await {
                error!("Droplets loading failed with err {e}");
                self.record_job_metrics("droplet_loading", false, start);
                continue;
            }
            self.droplet_store.record_droplets_metrics();

            self.record_job_metrics("droplet_loading", true, start)
        }
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
                let start = Instant::now();

                if let Err(e) = self.metrics_service.load_bandwidth().await {
                    self.record_job_metrics("bandwidth", false, start);
                    error!("Bandwidth metrics loading failed with err {e}");
                    continue;
                }
                self.record_job_metrics("bandwidth", true, start);
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
                let start = Instant::now();

                if let Err(e) = self.metrics_service.load_cpu_metrics().await {
                    self.record_job_metrics("cpu", false, start);
                    error!("Cpu metrics loading failed with err {e}");
                    continue;
                }
                self.record_job_metrics("cpu", true, start);
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
                let start = Instant::now();

                if let Err(e) = self.metrics_service.load_filesystem_metrics().await {
                    error!("Filesystem metrics loading failed with err {e}");
                    self.record_job_metrics("filesystem", false, start);
                    continue;
                }
                self.record_job_metrics("filesystem", true, start);
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
                let start = Instant::now();

                if let Err(e) = self.metrics_service.load_memory_metrics().await {
                    error!("Memory metrics loading failed with err {e}");
                    self.record_job_metrics("memory", false, start);
                    continue;
                }
                self.record_job_metrics("memory", true, start);
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
                let start = Instant::now();

                // load load =(
                if let Err(e) = self.metrics_service.load_load_metrics().await {
                    error!("Load metrics loading failed with err {e}");
                    self.record_job_metrics("load", false, start);
                    continue;
                }
                self.record_job_metrics("load", true, start);
            }
        }
        Ok(())
    }

    async fn run_agent_metrics_loading(&self) -> anyhow::Result<()> {
        if !self.configs.agent_metrics.enabled {
            info!("Agent metrics are disabled");
            return Ok(());
        }
        info!("Staring load agent metrics loop");
        // timeout for initial load
        // looks ugly, but simple =)
        let first_delay = Duration::from_secs(10).min(self.configs.agent_metrics.interval);
        let mut first = true;
        loop {
            let timeout = if first { first_delay } else { self.configs.agent_metrics.interval };
            first = false;
            tokio::time::sleep(timeout).await;

            // no metrics, because it always faster than 1 ms and should not be reason of troubles
            if let Err(e) = self.agent_service.load_agent_metrics() {
                error!("Load metrics loading failed with err {e}");
                continue;
            }
        }
    }
}











