mod config;
mod client;
mod metrics;


use std::ops::Deref;
use std::sync::Arc;
use poem::{handler, IntoResponse};
use poem::web::{Redirect};
use poem::{EndpointExt, Route, Server};
use poem::listener::{Listener, RustlsCertificate, RustlsConfig, TcpListener};
use prometheus::Registry;
use tracing::info;
use tracing::metadata::LevelFilter;
use crate::client::do_client::DigitalOceanClientImpl;
use crate::client::key_manager::{KeyManager, KeyManagerImpl};
use crate::config::config_model::{AgentMetricsConfigs, AppSettings};
use crate::metrics::agent_metrics::AgentMetricsImpl;
use crate::metrics::droplet_metrics_loader::DropletMetricsServiceImpl;
use crate::metrics::droplet_store::DropletStoreImpl;
use crate::metrics::jobs_scheduler::{MetricsScheduler, MetricsSchedulerImpl};

// because it breaks debugger =(
#[cfg(not(debug_assertions))]
#[global_allocator]
static GLOBAL_MIMALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

#[handler]
async fn prometheus_endpoint(request: &poem::Request,
                             registry: poem::web::Data<&prometheus::Registry>) -> poem::Result<String> {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = registry.deref().gather();
    let result = encoder.encode_to_string(&metric_families)
        .map_err(|x| anyhow::Error::from(x))?;
    Ok(result)
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (non_blocking, _guard) = tracing_appender::non_blocking(std::io::stdout());

    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::INFO)
        .with_writer(non_blocking)
        .init();

    let configs = config::parse::parse_configs("./config_example.yml".into())?;
    let configs: &'static _ = Box::leak(Box::new(configs.clone()));
    let registry = prometheus::Registry::new();

    let scheduler = build_app(registry.clone(), configs)?;
    let scheduler = Arc::new(scheduler);


    tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.run_droplets_loading().await }
    });
    tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.run_cpu_metrics_loading().await }
    });
    tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.run_filesystem_metrics_loading().await }
    });
    tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.run_bandwidth_metrics_loading().await }
    });
    tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.run_memory_metrics_loading().await }
    });
    tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.run_agent_metrics_loading().await }
    });

    let route = Route::new()
        .at("/metrics", poem::get(prometheus_endpoint))
        .data(registry);

    info!("Starting server");
    let bind_address = format!("0.0.0.0:8888");
    let listener = TcpListener::bind(bind_address);
    Server::new(listener)
        .run(route)
        .await?;

    Ok(())
}


fn build_app(registry: Registry,
             configs: &'static AppSettings) -> anyhow::Result<MetricsSchedulerImpl> {
    let key_manager = KeyManagerImpl::new(configs, registry.clone())?;
    let client = DigitalOceanClientImpl::new(
        configs,
        reqwest::Client::new(),
        Arc::new(key_manager),
        registry.clone()
    )?;
    let agent_metrics = AgentMetricsImpl::new(registry.clone());
    let droplets_store = DropletStoreImpl::new(
        Arc::new(client.clone()),
        configs,
        registry.clone(),
    );
    let droplets_metrics_loader = DropletMetricsServiceImpl::new(
        Arc::new(client.clone()),
        Arc::new(droplets_store.clone()),
        configs,
        registry.clone(),
    )?;

    let scheduler: MetricsSchedulerImpl = MetricsSchedulerImpl::new(
        Arc::new(client.clone()),
        configs,
        Arc::new(droplets_store.clone()),
        Arc::new(droplets_metrics_loader),
        Arc::new(agent_metrics),
        registry.clone(),
    )?;
    Ok(scheduler)
}