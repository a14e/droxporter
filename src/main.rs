mod config;
mod client;
mod metrics;


use std::fs;
use std::ops::Deref;
use std::sync::Arc;
use poem::handler;
use poem::{EndpointExt, Route, Server};
use poem::listener::{Listener, RustlsCertificate, RustlsConfig, TcpListener};
use prometheus::Registry;
use poem::web::{
    headers,
    headers::{authorization::Basic, HeaderMapExt},
};
use reqwest::StatusCode;
use tracing::info;
use tracing::metadata::LevelFilter;
use crate::client::do_client::DigitalOceanClientImpl;
use crate::client::key_manager::KeyManagerImpl;
use crate::config::config_model::{AppSettings, SslSettings};
use crate::metrics::agent_metrics::AgentMetricsImpl;
use crate::metrics::app_metrics_loader::AppMetricsServiceImpl;
use crate::metrics::droplet_metrics_loader::DropletMetricsServiceImpl;
use crate::metrics::droplet_store::DropletStoreImpl;
use crate::metrics::app_store::AppStoreImpl;
use crate::metrics::jobs_scheduler::{MetricsScheduler, MetricsSchedulerImpl};

// because it breaks debugger =(
#[cfg(not(debug_assertions))]
#[global_allocator]
static GLOBAL_MIMALLOC: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

#[handler]
async fn prometheus_endpoint(request: &poem::Request,
                             registry: poem::web::Data<&Registry>,
                             configs: poem::web::Data<&&'static AppSettings>) -> poem::Result<String> {
    // Simple basic auth check
    // I don't think that for a simple agent, it's worth using bcrypt or anything like that because:
    //   1. The information is not sensitive.
    //   2. It's easy to generate a long enough random password (60+ symbols), which should be sufficiently secure.
    //   3. The time required for this check is two orders of magnitude lower than the variations in network latency. Therefore, I believe a timing attack is not possible.
    if let Some(creds) = configs.endpoint.auth.as_ref().filter(|x| x.enabled) {
        if let Some(headers::Authorization(auth)) = request.headers().typed_get::<headers::Authorization<Basic>>() {
            if auth.username() != creds.login && auth.password() != creds.password {
                return Err(poem::Error::from_status(StatusCode::UNAUTHORIZED));
            }
        } else {
            return Err(poem::Error::from_status(StatusCode::UNAUTHORIZED));
        }
    }

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

    let configs = config::parse::parse_configs("./config.yml".into())?;
    let configs: &'static _ = Box::leak(Box::new(configs.clone()));
    let registry = {
        let trimmed_prefix = configs.custom.prefix.as_ref()
            .map(|x| x.as_str().trim())
            .filter(|x| !x.is_empty())
            .map(Into::into);
        let labels = configs.custom.labels.clone();
        let labels = Some(labels).filter(|x| !x.is_empty());
        Registry::new_custom(trimmed_prefix, labels)?
    };

    let scheduler = build_app(registry.clone(), configs)?;
    let scheduler = Arc::new(scheduler);


    tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.run_droplets_loading().await }
    });
    tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.run_apps_loading().await }
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
    tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.run_app_cpu_percentage_metrics_loading().await }
    });
    tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.run_app_memory_percentage_metrics_loading().await }
    });
    tokio::spawn({
        let scheduler = scheduler.clone();
        async move { scheduler.run_app_restart_count_metrics_loading().await }
    });

    let route = Route::new()
        .at("/metrics", poem::get(prometheus_endpoint))
        .data(registry)
        .data(configs);

    info!("Starting server");
    let bind_address = {
        let host = configs.endpoint.host.as_str();
        let port = configs.endpoint.port;
        format!("{host}:{port}")
    };
    let ssl_enabled = configs.endpoint.ssl.as_ref().map(|x| x.enabled).unwrap_or_default();
    if !ssl_enabled {
        info!("Ssl is disabled");
        let listener = TcpListener::bind(bind_address);
        Server::new(listener)
            .run(route)
            .await?;
    } else {
        info!("Ssl is enabled");
        let config = configs.endpoint.ssl.as_ref().unwrap_or_else(|| unreachable!());
        let config = create_poem_tls_config(config)?;
        let listener = TcpListener::bind(bind_address)
            .rustls(config);
        Server::new(listener)
            .run(route)
            .await?;
    }


    Ok(())
}


fn build_app(registry: Registry,
             configs: &'static AppSettings) -> anyhow::Result<MetricsSchedulerImpl> {
    let key_manager = KeyManagerImpl::new(configs, registry.clone())?;
    let client = DigitalOceanClientImpl::new(
        configs,
        reqwest::Client::new(),
        Arc::new(key_manager),
        registry.clone(),
    )?;
    let agent_metrics = AgentMetricsImpl::new(configs, registry.clone());
    let droplets_store = DropletStoreImpl::new(
        Arc::new(client.clone()),
        configs,
        registry.clone(),
    )?;
    let droplets_metrics_loader = DropletMetricsServiceImpl::new(
        Arc::new(client.clone()),
        Arc::new(droplets_store.clone()),
        configs,
        registry.clone(),
    )?;
    let app_store = AppStoreImpl::new(
        Arc::new(client.clone()),
        configs,
        registry.clone(),
    )?;
    let app_metrics_loader = AppMetricsServiceImpl::new(
        Arc::new(client.clone()),
        Arc::new(app_store.clone()),
        configs,
        registry.clone(),
    )?;

    let scheduler: MetricsSchedulerImpl = MetricsSchedulerImpl::new(
        configs,
        Arc::new(droplets_store.clone()),
        Arc::new(app_store.clone()),
        Arc::new(droplets_metrics_loader),
        Arc::new(app_metrics_loader),
        Arc::new(agent_metrics),
        registry.clone(),
    )?;
    Ok(scheduler)
}


fn create_poem_tls_config(config: &SslSettings) -> anyhow::Result<RustlsConfig> {
    let key = fs::read(config.key_path.as_str())?;
    let cert = fs::read(config.root_cert_path.as_str())?;
    let config = RustlsConfig::new()
        .fallback(RustlsCertificate::new().key(key).cert(cert));
    Ok(config)
}
