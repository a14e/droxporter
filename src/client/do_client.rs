use std::sync::Arc;
use std::time::Instant;
use async_trait::async_trait;
use chrono::Utc;
use prometheus::{HistogramOpts, Opts, Registry};
use reqwest::StatusCode;
use url::Url;
use crate::client::do_json_protocol::{DataResponse, ListDropletsResponse};
use crate::client::key_manager::{KeyManager, KeyType};
use crate::config::config_model::{AgentMetricsType, AppSettings};
use crate::metrics::utils::DROXPORTER_DEFAULT_BUCKETS;

#[async_trait]
pub trait DigitalOceanClient: Send + Sync {
    async fn list_droplets(&self,
                           per_page: u64,
                           page: u64) -> anyhow::Result<ListDropletsResponse>;


    async fn get_bandwidth(&self,
                           host_id: u64,
                           interface: NetworkInterface,
                           direction: NetworkDirection,
                           start: chrono::DateTime<Utc>,
                           end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse>;


    async fn get_cpu(&self,
                     host_id: u64,
                     start: chrono::DateTime<Utc>,
                     end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse>;


    async fn get_file_system(&self,
                             host_id: u64,
                             metric_type: FileSystemRequest,
                             start: chrono::DateTime<Utc>,
                             end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse>;


    async fn get_droplet_memory(&self,
                                host_id: u64,
                                metric_type: MemoryRequest,
                                start: chrono::DateTime<Utc>,
                                end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse>;


    async fn get_load(&self,
                      host_id: u64,
                      load_type: ClientLoadType,
                      start: chrono::DateTime<Utc>,
                      end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse>;
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
    pub fn new(config: &'static AppSettings,
               client: reqwest::Client,
               token_manager: Arc<dyn KeyManager>,
               registry: Registry) -> anyhow::Result<Self> {
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
    fn new(config: &'static AppSettings,
           registry: Registry) -> anyhow::Result<Self> {
        let requests_counter = prometheus::CounterVec::new(
            Opts::new("droxporter_digital_ocean_request_counter", "Counter of droxporter http request"),
            &["type", "result"],
        )?;
        let request_histogram = prometheus::HistogramVec::new(
            HistogramOpts::new("droxporter_digital_ocean_request_histogram_seconds", "Time of droxporter http request")
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
        self.config.agent_metrics.enabled && {
            self.config.agent_metrics.metrics.contains(&AgentMetricsType::Requests)
        }
    }

    fn record_client_metrics(&self,
                             request: &str,
                             response_code: &str,
                             start_time: Instant) {
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
    async fn base_metrics_request(&self,
                                  request_type: RequestType,
                                  host_id: u64,
                                  start: chrono::DateTime<Utc>,
                                  end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        let suffix = request_type.to_request_suffix()?;
        let mut url = {
            let base = self.config.metrics.base_url.as_str();
            let str = format!("{base}/{suffix}"); // or path_segments_mut?
            Url::parse(str.as_str())?
        };

        url.query_pairs_mut()
            .append_pair("host_id", host_id.to_string().as_str())
            .append_pair("start", start.timestamp().to_string().as_str())
            .append_pair("end", end.timestamp().to_string().as_str());


        let bearer = self.token_manager.acquire_key(request_type.into())?;
        let time = Instant::now();

        let response = self.client
            .get(url)
            .bearer_auth(bearer)
            .send()
            .await?;

        self.metrics.record_client_metrics(suffix, response.status().as_str(), time);

        if response.status() != StatusCode::OK && response.status() != StatusCode::NO_CONTENT {
            let status = response.status();
            let body = response.text().await?;
            let err = format!("Request failed with status code: {status}, body: {body}");
            return Err(anyhow::Error::msg(err));
        }

        let res = response.json::<DataResponse>().await?;


        Ok(res)
    }
}


#[derive(PartialEq, Eq, Hash, Copy, Clone)]
pub enum RequestType {
    Droplets,
    Bandwidth,
    Cpu,
    FileSystemFree,
    FileSystemSize,
    CachedMemory,
    FreeMemory,
    TotalMemory,
    AvailableTotalMemory,
    Load1,
    Load5,
    Load15,
}

#[derive(Clone, Copy)]
pub enum FileSystemRequest {
    Free,
    Size,
}

#[derive(Clone, Copy)]
pub enum MemoryRequest {
    CachedMemory,
    FreeMemory,
    TotalMemory,
    AvailableTotalMemory,
}


impl RequestType {
    pub fn to_request_suffix(self) -> anyhow::Result<&'static str> {
        match self {
            RequestType::Bandwidth => Ok("bandwidth"),
            RequestType::Cpu => Ok("cpu"),
            RequestType::FileSystemFree => Ok("filesystem_free"),
            RequestType::FileSystemSize => Ok("filesystem_size"),
            RequestType::CachedMemory => Ok("memory_cached"),
            RequestType::FreeMemory => Ok("memory_free"),
            RequestType::TotalMemory => Ok("memory_total"),
            RequestType::AvailableTotalMemory => Ok("memory_available"),
            RequestType::Load1 => Ok("load_1"),
            RequestType::Load5 => Ok("load_5"),
            RequestType::Load15 => Ok("load_15"),
            _ => anyhow::bail!("Unexpected key type")
        }
    }
}

impl Into<KeyType> for RequestType {
    fn into(self) -> KeyType {
        match self {
            RequestType::Droplets => KeyType::Droplets,
            RequestType::Bandwidth => KeyType::Bandwidth,
            RequestType::Cpu => KeyType::Cpu,
            RequestType::FileSystemFree => KeyType::FileSystem,
            RequestType::FileSystemSize => KeyType::FileSystem,
            RequestType::CachedMemory => KeyType::Memory,
            RequestType::FreeMemory => KeyType::Memory,
            RequestType::TotalMemory => KeyType::Memory,
            RequestType::AvailableTotalMemory => KeyType::Memory,
            RequestType::Load1 => KeyType::Load,
            RequestType::Load5 => KeyType::Load,
            RequestType::Load15 => KeyType::Load,
        }
    }
}

#[async_trait]
impl DigitalOceanClient for DigitalOceanClientImpl {
    async fn list_droplets(&self,
                           per_page: u64,
                           page: u64) -> anyhow::Result<ListDropletsResponse> {
        let mut url = Url::parse(self.config.droplets.url.as_str())?;
        url.query_pairs_mut()
            .append_pair("per_page", per_page.to_string().as_str())
            .append_pair("page", page.to_string().as_str());

        let bearer = self.token_manager.acquire_key(RequestType::Droplets.into())?;
        let time = Instant::now();

        let response = self.client
            .get(url)
            .bearer_auth(bearer)
            .send()
            .await?;
        self.metrics.record_client_metrics("list_droplets", response.status().as_str(), time);

        if response.status() != StatusCode::OK && response.status() != StatusCode::NO_CONTENT {
            let status = response.status();
            let body = response.text().await?;
            let err = format!("Request failed with status code: {status}, body: {body}");
            return Err(anyhow::Error::msg(err));
        }

        let res = response.json::<ListDropletsResponse>()
            .await?;


        Ok(res)
    }

    async fn get_bandwidth(&self,
                           host_id: u64,
                           interface: NetworkInterface,
                           direction: NetworkDirection,
                           start: chrono::DateTime<Utc>,
                           end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        let mut url = {
            let base = self.config.metrics.base_url.as_str();
            let str = format!("{base}/bandwidth"); // or path_segments_mut?
            Url::parse(str.as_str())?
        };

        let interface = if interface == NetworkInterface::Private { "private" } else { "public" };
        let direction = if direction == NetworkDirection::Inbound { "inbound" } else { "outbound" };

        url.query_pairs_mut()
            .append_pair("host_id", host_id.to_string().as_str())
            .append_pair("interface", interface)
            .append_pair("direction", direction)
            .append_pair("start", start.timestamp().to_string().as_str())
            .append_pair("end", end.timestamp().to_string().as_str());

        let bearer = self.token_manager.acquire_key(RequestType::Bandwidth.into())?;
        let time = Instant::now();

        let response = self.client
            .get(url)
            .bearer_auth(bearer)
            .send()
            .await?;

        self.metrics.record_client_metrics("bandwidth", response.status().as_str(), time);

        if response.status() != StatusCode::OK && response.status() != StatusCode::NO_CONTENT {
            let status = response.status();
            let body = response.text().await?;
            let err = format!("Request failed with status code: {status}, body: {body}");
            return Err(anyhow::Error::msg(err));
        }

        let res = response.json::<DataResponse>().await?;

        Ok(res)
    }

    async fn get_cpu(&self,
                     host_id: u64,
                     start: chrono::DateTime<Utc>,
                     end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        self.base_metrics_request(
            RequestType::Cpu,
            host_id,
            start,
            end,
        ).await
    }

    async fn get_file_system(&self,
                             host_id: u64,
                             request: FileSystemRequest,
                             start: chrono::DateTime<Utc>,
                             end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        let request = match request {
            FileSystemRequest::Free => RequestType::FileSystemFree,
            FileSystemRequest::Size => RequestType::FileSystemSize
        };
        self.base_metrics_request(
            request,
            host_id,
            start,
            end,
        ).await
    }


    async fn get_droplet_memory(&self,
                                host_id: u64,
                                metric_type: MemoryRequest,
                                start: chrono::DateTime<Utc>,
                                end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        let request_type = match metric_type {
            MemoryRequest::CachedMemory => RequestType::CachedMemory,
            MemoryRequest::FreeMemory => RequestType::FreeMemory,
            MemoryRequest::TotalMemory => RequestType::TotalMemory,
            MemoryRequest::AvailableTotalMemory => RequestType::AvailableTotalMemory,
        };

        self.base_metrics_request(
            request_type,
            host_id,
            start,
            end,
        ).await
    }


    async fn get_load(&self,
                      host_id: u64,
                      load_type: ClientLoadType,
                      start: chrono::DateTime<Utc>,
                      end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        let request_type = match load_type {
            ClientLoadType::Load1 => RequestType::Load1,
            ClientLoadType::Load5 => RequestType::Load5,
            ClientLoadType::Load15 => RequestType::Load15,
        };

        self.base_metrics_request(
            request_type,
            host_id,
            start,
            end,
        ).await
    }
}