use crate::client::do_json_protocol::{
    DropletDataResponse, ListAppsResponse, ListDropletsResponse,
};
use crate::client::key_manager::{KeyManager, KeyType};
use crate::config::config_model::{AgentMetricsType, AppSettings};
use crate::metrics::utils::DROXPORTER_DEFAULT_BUCKETS;
use async_trait::async_trait;
use chrono::Utc;
use prometheus::{HistogramOpts, Opts, Registry};
use reqwest::StatusCode;
use std::sync::Arc;
use std::time::Instant;
use url::Url;

use super::do_json_protocol::AppDataResponse;

#[async_trait]
pub trait DigitalOceanClient: Send + Sync {
    async fn list_droplets(&self, per_page: u64, page: u64)
        -> anyhow::Result<ListDropletsResponse>;

    async fn list_apps(&self, per_page: u64, page: u64) -> anyhow::Result<ListAppsResponse>;

    async fn get_droplet_bandwidth(
        &self,
        host_id: u64,
        interface: NetworkInterface,
        direction: NetworkDirection,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<DropletDataResponse>;

    async fn get_droplet_cpu(
        &self,
        host_id: u64,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<DropletDataResponse>;

    async fn get_droplet_file_system(
        &self,
        host_id: u64,
        metric_type: FileSystemRequest,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<DropletDataResponse>;

    async fn get_droplet_memory(
        &self,
        host_id: u64,
        metric_type: MemoryRequest,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<DropletDataResponse>;

    async fn get_droplet_load(
        &self,
        host_id: u64,
        load_type: ClientLoadType,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<DropletDataResponse>;

    async fn get_app_cpu_percentage(
        &self,
        app_id: String,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<AppDataResponse>;

    async fn get_app_memory_percentage(
        &self,
        app_id: String,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<AppDataResponse>;

    async fn get_app_restart_count(
        &self,
        app_id: String,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<AppDataResponse>;
}

#[derive(Eq, PartialEq, Copy, Clone)]
pub enum NetworkDirection {
    Inbound,
    Outbound,
}

#[derive(Eq, PartialEq, Copy, Clone)]
pub enum NetworkInterface {
    Public,
    Private,
}

#[derive(Eq, PartialEq, Copy, Clone)]
pub enum ClientLoadType {
    Load1,
    Load5,
    Load15,
}

#[derive(Clone)]
pub struct DigitalOceanClientImpl {
    config: &'static AppSettings,
    client: reqwest::Client,
    token_manager: Arc<dyn KeyManager>,
    metrics: DigitalOceanClientMetrics,
}

impl DigitalOceanClientImpl {
    pub fn new(
        config: &'static AppSettings,
        client: reqwest::Client,
        token_manager: Arc<dyn KeyManager>,
        registry: Registry,
    ) -> anyhow::Result<Self> {
        let result = Self {
            config,
            client,
            token_manager,
            metrics: DigitalOceanClientMetrics::new(config, registry)?,
        };
        Ok(result)
    }
}

#[derive(Clone)]
struct DigitalOceanClientMetrics {
    config: &'static AppSettings,
    requests_counter: prometheus::CounterVec,
    request_histogram: prometheus::HistogramVec,
}

impl DigitalOceanClientMetrics {
    fn new(config: &'static AppSettings, registry: Registry) -> anyhow::Result<Self> {
        let requests_counter = prometheus::CounterVec::new(
            Opts::new(
                "droxporter_digital_ocean_request_counter",
                "Counter of droxporter http request",
            ),
            &["type", "result"],
        )?;
        let request_histogram = prometheus::HistogramVec::new(
            HistogramOpts::new(
                "droxporter_digital_ocean_request_histogram_seconds",
                "Time of droxporter http request",
            )
            .buckets((*DROXPORTER_DEFAULT_BUCKETS).into()),
            &["type", "result"],
        )?;
        registry.register(Box::new(requests_counter.clone()))?;
        registry.register(Box::new(request_histogram.clone()))?;
        let result = Self {
            config,
            requests_counter,
            request_histogram,
        };
        Ok(result)
    }

    fn is_enabled(&self) -> bool {
        self.config.exporter_metrics.enabled && {
            self.config
                .exporter_metrics
                .metrics
                .contains(&AgentMetricsType::Requests)
        }
    }

    fn record_client_metrics(&self, request: &str, response_code: &str, start_time: Instant) {
        if !self.is_enabled() {
            return;
        }

        let elasped_time_seconds = start_time.elapsed().as_millis() as f64 / 1000.0f64;

        self.request_histogram
            .with_label_values(&[request, response_code])
            .observe(elasped_time_seconds);
        self.requests_counter
            .with_label_values(&[request, response_code])
            .inc();
    }
}

impl DigitalOceanClientImpl {
    async fn base_droplet_metrics_request(
        &self,
        request_type: RequestType,
        host_id: u64,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<DropletDataResponse> {
        let suffix = request_type.to_request_suffix()?;
        let mut url = {
            let base = self.config.droplet_metrics.base_url.as_str();
            let str = format!("{base}/{suffix}"); // or path_segments_mut?
            Url::parse(str.as_str())?
        };

        url.query_pairs_mut()
            .append_pair("host_id", host_id.to_string().as_str())
            .append_pair("start", start.timestamp().to_string().as_str())
            .append_pair("end", end.timestamp().to_string().as_str());

        let bearer = self.token_manager.acquire_key(request_type.into())?;
        let time = Instant::now();

        let response = self.client.get(url).bearer_auth(bearer).send().await?;

        self.metrics
            .record_client_metrics(suffix, response.status().as_str(), time);

        if response.status() != StatusCode::OK && response.status() != StatusCode::NO_CONTENT {
            let status = response.status();
            let body = response.text().await?;
            let err = format!("Request failed with status code: {status}, body: {body}");
            return Err(anyhow::Error::msg(err));
        }

        let res = response.json::<DropletDataResponse>().await?;

        Ok(res)
    }

    async fn base_app_metrics_request(
        &self,
        request_type: RequestType,
        app_id: String,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<AppDataResponse> {
        let suffix = request_type.to_request_suffix()?;
        let mut url = {
            let base = self.config.app_metrics.base_url.as_str();
            let str = format!("{base}/{suffix}"); // or path_segments_mut?
            Url::parse(str.as_str())?
        };

        url.query_pairs_mut()
            .append_pair("app_id", app_id.as_str())
            .append_pair("start", start.timestamp().to_string().as_str())
            .append_pair("end", end.timestamp().to_string().as_str());

        let bearer = self.token_manager.acquire_key(request_type.into())?;
        let time = Instant::now();

        let response = self.client.get(url).bearer_auth(bearer).send().await?;

        self.metrics
            .record_client_metrics(suffix, response.status().as_str(), time);

        if response.status() != StatusCode::OK && response.status() != StatusCode::NO_CONTENT {
            let status = response.status();
            let body = response.text().await?;
            let err = format!("Request failed with status code: {status}, body: {body}");
            return Err(anyhow::Error::msg(err));
        }

        let res = response.json::<AppDataResponse>().await?;

        Ok(res)
    }
}

#[derive(PartialEq, Eq, Hash, Copy, Clone, Debug)]
pub enum RequestType {
    Droplets,
    Apps,
    DropletBandwidth,
    DropletCpu,
    DropletFileSystemFree,
    DropletFileSystemSize,
    DropletCachedMemory,
    DropletFreeMemory,
    DropletTotalMemory,
    DropletAvailableTotalMemory,
    DropletLoad1,
    DropletLoad5,
    DropletLoad15,
    AppCpuPercentage,
    AppMemoryPercentage,
    AppRestartCount,
}

#[derive(Clone, Copy)]
pub enum FileSystemRequest {
    Free,
    Size,
}

#[derive(Clone, Copy)]
pub enum MemoryRequest {
    Cached,
    Free,
    Total,
    AvailableTotal,
}

impl RequestType {
    pub fn to_request_suffix(self) -> anyhow::Result<&'static str> {
        match self {
            RequestType::DropletBandwidth => Ok("bandwidth"),
            RequestType::DropletCpu => Ok("cpu"),
            RequestType::DropletFileSystemFree => Ok("filesystem_free"),
            RequestType::DropletFileSystemSize => Ok("filesystem_size"),
            RequestType::DropletCachedMemory => Ok("memory_cached"),
            RequestType::DropletFreeMemory => Ok("memory_free"),
            RequestType::DropletTotalMemory => Ok("memory_total"),
            RequestType::DropletAvailableTotalMemory => Ok("memory_available"),
            RequestType::DropletLoad1 => Ok("load_1"),
            RequestType::DropletLoad5 => Ok("load_5"),
            RequestType::DropletLoad15 => Ok("load_15"),
            RequestType::AppCpuPercentage => Ok("cpu_percentage"),
            RequestType::AppMemoryPercentage => Ok("memory_percentage"),
            RequestType::AppRestartCount => Ok("restart_count"),
            _ => anyhow::bail!("Unexpected key type"),
        }
    }
}

impl From<RequestType> for KeyType {
    fn from(val: RequestType) -> Self {
        match val {
            RequestType::Droplets => KeyType::Droplets,
            RequestType::Apps => KeyType::Apps,
            RequestType::DropletBandwidth => KeyType::DropletBandwidth,
            RequestType::DropletCpu => KeyType::DropletCpu,
            RequestType::DropletFileSystemFree => KeyType::DropletFileSystem,
            RequestType::DropletFileSystemSize => KeyType::DropletFileSystem,
            RequestType::DropletCachedMemory => KeyType::DropletMemory,
            RequestType::DropletFreeMemory => KeyType::DropletMemory,
            RequestType::DropletTotalMemory => KeyType::DropletMemory,
            RequestType::DropletAvailableTotalMemory => KeyType::DropletMemory,
            RequestType::DropletLoad1 => KeyType::DropletLoad,
            RequestType::DropletLoad5 => KeyType::DropletLoad,
            RequestType::DropletLoad15 => KeyType::DropletLoad,
            RequestType::AppCpuPercentage => KeyType::AppCpuPercentage,
            RequestType::AppMemoryPercentage => KeyType::AppMemoryPercentage,
            RequestType::AppRestartCount => KeyType::AppRestartCount,
        }
    }
}

#[async_trait]
impl DigitalOceanClient for DigitalOceanClientImpl {
    async fn list_droplets(
        &self,
        per_page: u64,
        page: u64,
    ) -> anyhow::Result<ListDropletsResponse> {
        let mut url = Url::parse(self.config.droplets.url.as_str())?;
        url.query_pairs_mut()
            .append_pair("per_page", per_page.to_string().as_str())
            .append_pair("page", page.to_string().as_str());

        let bearer = self
            .token_manager
            .acquire_key(RequestType::Droplets.into())?;
        let time = Instant::now();

        let response = self.client.get(url).bearer_auth(bearer).send().await?;
        self.metrics
            .record_client_metrics("list_droplets", response.status().as_str(), time);

        if response.status() != StatusCode::OK && response.status() != StatusCode::NO_CONTENT {
            let status = response.status();
            let body = response.text().await?;
            let err = format!("Request failed with status code: {status}, body: {body}");
            return Err(anyhow::Error::msg(err));
        }

        let res = response.json::<ListDropletsResponse>().await?;

        Ok(res)
    }

    async fn list_apps(&self, per_page: u64, page: u64) -> anyhow::Result<ListAppsResponse> {
        let mut url = Url::parse(self.config.apps.url.as_str())?;
        url.query_pairs_mut()
            .append_pair("per_page", per_page.to_string().as_str())
            .append_pair("page", page.to_string().as_str());

        let bearer = self.token_manager.acquire_key(RequestType::Apps.into())?;
        let time = Instant::now();

        let response = self.client.get(url).bearer_auth(bearer).send().await?;
        self.metrics
            .record_client_metrics("list_apps", response.status().as_str(), time);

        if response.status() != StatusCode::OK && response.status() != StatusCode::NO_CONTENT {
            let status = response.status();
            let body = response.text().await?;
            let err = format!("Request failed with status code: {status}, body: {body}");
            return Err(anyhow::Error::msg(err));
        }

        let res = response.json::<ListAppsResponse>().await?;

        Ok(res)
    }

    async fn get_droplet_bandwidth(
        &self,
        host_id: u64,
        interface: NetworkInterface,
        direction: NetworkDirection,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<DropletDataResponse> {
        let mut url = {
            let base = self.config.droplet_metrics.base_url.as_str();
            let str = format!("{base}/bandwidth"); // or path_segments_mut?
            Url::parse(str.as_str())?
        };

        let interface = if interface == NetworkInterface::Private {
            "private"
        } else {
            "public"
        };
        let direction = if direction == NetworkDirection::Inbound {
            "inbound"
        } else {
            "outbound"
        };

        url.query_pairs_mut()
            .append_pair("host_id", host_id.to_string().as_str())
            .append_pair("interface", interface)
            .append_pair("direction", direction)
            .append_pair("start", start.timestamp().to_string().as_str())
            .append_pair("end", end.timestamp().to_string().as_str());

        let bearer = self
            .token_manager
            .acquire_key(RequestType::DropletBandwidth.into())?;
        let time = Instant::now();

        let response = self.client.get(url).bearer_auth(bearer).send().await?;

        self.metrics
            .record_client_metrics("bandwidth", response.status().as_str(), time);

        if response.status() != StatusCode::OK && response.status() != StatusCode::NO_CONTENT {
            let status = response.status();
            let body = response.text().await?;
            let err = format!("Request failed with status code: {status}, body: {body}");
            return Err(anyhow::Error::msg(err));
        }

        let res = response.json::<DropletDataResponse>().await?;

        Ok(res)
    }

    async fn get_droplet_cpu(
        &self,
        host_id: u64,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<DropletDataResponse> {
        self.base_droplet_metrics_request(RequestType::DropletCpu, host_id, start, end)
            .await
    }

    async fn get_droplet_file_system(
        &self,
        host_id: u64,
        request: FileSystemRequest,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<DropletDataResponse> {
        let request = match request {
            FileSystemRequest::Free => RequestType::DropletFileSystemFree,
            FileSystemRequest::Size => RequestType::DropletFileSystemSize,
        };
        self.base_droplet_metrics_request(request, host_id, start, end)
            .await
    }

    async fn get_droplet_memory(
        &self,
        host_id: u64,
        metric_type: MemoryRequest,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<DropletDataResponse> {
        let request_type = match metric_type {
            MemoryRequest::Cached => RequestType::DropletCachedMemory,
            MemoryRequest::Free => RequestType::DropletFreeMemory,
            MemoryRequest::Total => RequestType::DropletTotalMemory,
            MemoryRequest::AvailableTotal => RequestType::DropletAvailableTotalMemory,
        };

        self.base_droplet_metrics_request(request_type, host_id, start, end)
            .await
    }

    async fn get_droplet_load(
        &self,
        host_id: u64,
        load_type: ClientLoadType,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<DropletDataResponse> {
        let request_type = match load_type {
            ClientLoadType::Load1 => RequestType::DropletLoad1,
            ClientLoadType::Load5 => RequestType::DropletLoad5,
            ClientLoadType::Load15 => RequestType::DropletLoad15,
        };

        self.base_droplet_metrics_request(request_type, host_id, start, end)
            .await
    }

    async fn get_app_cpu_percentage(
        &self,
        app_id: String,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<AppDataResponse> {
        self.base_app_metrics_request(RequestType::AppCpuPercentage, app_id, start, end)
            .await
    }

    async fn get_app_memory_percentage(
        &self,
        app_id: String,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<AppDataResponse> {
        self.base_app_metrics_request(RequestType::AppMemoryPercentage, app_id, start, end)
            .await
    }

    async fn get_app_restart_count(
        &self,
        app_id: String,
        start: chrono::DateTime<Utc>,
        end: chrono::DateTime<Utc>,
    ) -> anyhow::Result<AppDataResponse> {
        self.base_app_metrics_request(RequestType::AppRestartCount, app_id, start, end)
            .await
    }
}
