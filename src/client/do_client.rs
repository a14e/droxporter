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

#[cfg_attr(test, mockall::automock)]
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

    #[allow(dead_code)]
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
#[allow(dead_code)]
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
    #[allow(dead_code)]
    DropletLoad1,
    #[allow(dead_code)]
    DropletLoad5,
    #[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::key_manager::KeyManagerImpl;
    use crate::config::config_model::AppSettings;
    use mockito::Server;
    use std::time::Duration;

    fn create_test_config(server_url: &str) -> &'static AppSettings {
        let config = AppSettings {
            default_keys: vec!["test-api-key".to_string()],
            droplets: crate::config::config_model::DropletSettings {
                keys: vec![],
                url: format!("{}/v2/droplets", server_url),
                interval: Duration::from_secs(60),
                metrics: vec![],
            },
            apps: crate::config::config_model::AppPlatformSettings {
                keys: vec![],
                url: format!("{}/v2/apps", server_url),
                interval: Duration::from_secs(60),
                metrics: vec![],
            },
            droplet_metrics: crate::config::config_model::DropletMetricsConfig {
                base_url: format!("{}/v2/monitoring/metrics/droplet", server_url),
                bandwidth: Some(crate::config::config_model::BandwidthSettings {
                    enabled: true,
                    interval: Duration::from_secs(60),
                    types: vec![],
                    keys: vec![],
                }),
                cpu: Some(crate::config::config_model::CpuSettings {
                    enabled: true,
                    interval: Duration::from_secs(60),
                    keys: vec![],
                }),
                filesystem: Some(crate::config::config_model::FilesystemSettings {
                    enabled: true,
                    interval: Duration::from_secs(60),
                    types: vec![],
                    keys: vec![],
                }),
                memory: Some(crate::config::config_model::MemorySettings {
                    enabled: false,
                    interval: Duration::from_secs(60),
                    types: vec![],
                    keys: vec![],
                }),
                load: None,
            },
            app_metrics: crate::config::config_model::AppMetricsConfig {
                base_url: format!("{}/v2/monitoring/metrics/apps", server_url),
                cpu_percentage: Some(crate::config::config_model::AppCpuPercentageSettings {
                    enabled: true,
                    interval: Duration::from_secs(60),
                    keys: vec![],
                }),
                memory_percentage: Some(crate::config::config_model::AppMemoryPercentageSettings {
                    enabled: true,
                    interval: Duration::from_secs(60),
                    keys: vec![],
                }),
                restart_count: Some(crate::config::config_model::AppRestartCountSettings {
                    enabled: true,
                    interval: Duration::from_secs(60),
                    keys: vec![],
                }),
            },
            exporter_metrics: crate::config::config_model::ExporterMetricsConfigs {
                enabled: false,
                interval: Duration::from_secs(60),
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
    async fn test_list_droplets_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v2/droplets?per_page=100&page=1")
            .match_header("authorization", "Bearer test-api-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"droplets":[{"id":123,"name":"test-droplet","memory":2048,"vcpus":1,"disk":50,"locked":false,"status":"active"}],"links":{"pages":{}}}"#)
            .create_async()
            .await;

        let config = create_test_config(&server.url());
        let client = reqwest::Client::new();
        let key_registry = prometheus::Registry::new();
        let key_manager = KeyManagerImpl::new(config, key_registry).unwrap();
        let registry = prometheus::Registry::new();

        let do_client =
            DigitalOceanClientImpl::new(config, client, Arc::new(key_manager), registry).unwrap();

        let result = do_client.list_droplets(100, 1).await;
        mock.assert_async().await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.droplets.len(), 1);
        assert_eq!(response.droplets[0].id, 123);
        assert_eq!(response.droplets[0].name, "test-droplet");
    }

    #[tokio::test]
    async fn test_list_droplets_http_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v2/droplets?per_page=100&page=1")
            .match_header("authorization", "Bearer test-api-key")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"message":"Unauthorized"}"#)
            .create_async()
            .await;

        let config = create_test_config(&server.url());
        let client = reqwest::Client::new();
        let key_registry = prometheus::Registry::new();
        let key_manager = KeyManagerImpl::new(config, key_registry).unwrap();
        let registry = prometheus::Registry::new();

        let do_client =
            DigitalOceanClientImpl::new(config, client, Arc::new(key_manager), registry).unwrap();

        let result = do_client.list_droplets(100, 1).await;
        mock.assert_async().await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("401"));
    }

    #[tokio::test]
    async fn test_get_droplet_cpu_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Regex(r"^/v2/monitoring/metrics/droplet/cpu\?host_id=123&start=\d+&end=\d+$".to_string()))
            .match_header("authorization", "Bearer test-api-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"success","data":{"result":[{"metric":{"host_id":"123","mode":"idle"},"values":[[1682246520,"95.5"]]}]}}"#)
            .create_async()
            .await;

        let config = create_test_config(&server.url());
        let client = reqwest::Client::new();
        let key_registry = prometheus::Registry::new();
        let key_manager = KeyManagerImpl::new(config, key_registry).unwrap();
        let registry = prometheus::Registry::new();

        let do_client =
            DigitalOceanClientImpl::new(config, client, Arc::new(key_manager), registry).unwrap();

        let start = chrono::Utc::now() - chrono::Duration::minutes(5);
        let end = chrono::Utc::now();

        let result = do_client.get_droplet_cpu(123, start, end).await;
        mock.assert_async().await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status, "success");
        assert_eq!(response.data.result.len(), 1);
    }

    #[tokio::test]
    async fn test_get_droplet_bandwidth_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Regex(r"^/v2/monitoring/metrics/droplet/bandwidth\?host_id=123&interface=public&direction=inbound&start=\d+&end=\d+$".to_string()))
            .match_header("authorization", "Bearer test-api-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"success","data":{"result":[{"metric":{"host_id":"123"},"values":[[1682246520,"1024.5"]]}]}}"#)
            .create_async()
            .await;

        let config = create_test_config(&server.url());
        let client = reqwest::Client::new();
        let key_registry = prometheus::Registry::new();
        let key_manager = KeyManagerImpl::new(config, key_registry).unwrap();
        let registry = prometheus::Registry::new();

        let do_client =
            DigitalOceanClientImpl::new(config, client, Arc::new(key_manager), registry).unwrap();

        let start = chrono::Utc::now() - chrono::Duration::minutes(5);
        let end = chrono::Utc::now();

        let result = do_client
            .get_droplet_bandwidth(
                123,
                NetworkInterface::Public,
                NetworkDirection::Inbound,
                start,
                end,
            )
            .await;
        mock.assert_async().await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status, "success");
    }

    #[tokio::test]
    async fn test_list_apps_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/v2/apps?per_page=50&page=1")
            .match_header("authorization", "Bearer test-api-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"apps":[{"id":"app-123","spec":{"name":"test-app"},"active_deployment":{"id":"dep-1","cause":"manual","phase":"ACTIVE"}}],"links":{"pages":{}}}"#)
            .create_async()
            .await;

        let config = create_test_config(&server.url());
        let client = reqwest::Client::new();
        let key_registry = prometheus::Registry::new();
        let key_manager = KeyManagerImpl::new(config, key_registry).unwrap();
        let registry = prometheus::Registry::new();

        let do_client =
            DigitalOceanClientImpl::new(config, client, Arc::new(key_manager), registry).unwrap();

        let result = do_client.list_apps(50, 1).await;
        mock.assert_async().await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.apps.len(), 1);
        assert_eq!(response.apps[0].id, "app-123");
        assert_eq!(response.apps[0].spec.name, "test-app");
    }

    #[tokio::test]
    async fn test_get_droplet_memory_success() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", mockito::Matcher::Regex(r"^/v2/monitoring/metrics/droplet/memory_free\?host_id=456&start=\d+&end=\d+$".to_string()))
            .match_header("authorization", "Bearer test-api-key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"success","data":{"result":[{"metric":{"host_id":"456"},"values":[[1682246520,"512000000"]]}]}}"#)
            .create_async()
            .await;

        let config = create_test_config(&server.url());
        let client = reqwest::Client::new();
        let key_registry = prometheus::Registry::new();
        let key_manager = KeyManagerImpl::new(config, key_registry).unwrap();
        let registry = prometheus::Registry::new();

        let do_client =
            DigitalOceanClientImpl::new(config, client, Arc::new(key_manager), registry).unwrap();

        let start = chrono::Utc::now() - chrono::Duration::minutes(30);
        let end = chrono::Utc::now();

        let result = do_client
            .get_droplet_memory(456, MemoryRequest::Free, start, end)
            .await;
        mock.assert_async().await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status, "success");
        assert_eq!(response.data.result.len(), 1);
    }
}
