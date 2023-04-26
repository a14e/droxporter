use async_trait::async_trait;
use chrono::Utc;
use url::Url;
use crate::client::do_json_protocol::{DataResponse, ListDropletsResponse};
use crate::client::key_manager::{KeyManager, KeyType};
use crate::config::config_model::AppSettings;

#[async_trait]
pub trait DigitalOceanClient {
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


    async fn get_file_system_free(&self,
                                  host_id: u64,
                                  start: chrono::DateTime<Utc>,
                                  end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse>;

    async fn get_file_system_size(&self,
                                  host_id: u64,
                                  start: chrono::DateTime<Utc>,
                                  end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse>;


    async fn get_droplet_cached_memory(&self,
                                       host_id: u64,
                                       start: chrono::DateTime<Utc>,
                                       end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse>;


    async fn get_droplet_free_memory(&self,
                                     host_id: u64,
                                     start: chrono::DateTime<Utc>,
                                     end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse>;

    async fn get_droplet_total_memory(&self,
                                      host_id: u64,
                                      start: chrono::DateTime<Utc>,
                                      end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse>;

    async fn get_available_total_memory(&self,
                                        host_id: u64,
                                        start: chrono::DateTime<Utc>,
                                        end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse>;


    async fn get_load(&self,
                      host_id: u64,
                      load_type: LoadType,
                      start: chrono::DateTime<Utc>,
                      end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse>;
}


#[derive(Eq, PartialEq)]
pub enum NetworkDirection {
    Inbound,
    Outbound,
}

#[derive(Eq, PartialEq)]
pub enum NetworkInterface {
    Public,
    Private,
}

#[derive(Eq, PartialEq)]
pub enum LoadType {
    Load1,
    Load5,
    Load15,
}


#[derive(mydi::Component, Clone)]
pub struct DigitalOceanClientImpl<KeyManager: 'static> {
    config: &'static AppSettings,
    client: reqwest::Client,
    token_manager: KeyManager,
}


impl<T: KeyManager> DigitalOceanClientImpl<T> {
    async fn base_metrics_request(&self,
                                  request_type: RequestType,
                                  host_id: u64,
                                  start: chrono::DateTime<Utc>,
                                  end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        let mut url = {
            let base = self.config.metrics.base_url.as_str();
            let suffix = request_type.to_request_suffix()?;
            let str = format!("{base}/{suffix}"); // or path_segments_mut?
            Url::parse(str.as_str())?
        };

        url.query_pairs_mut()
            .append_pair("host_id", host_id.to_string().as_str())
            .append_pair("start", start.timestamp().to_string().as_str())
            .append_pair("end", end.timestamp().to_string().as_str());


        let bearer = self.token_manager.acquire_key(request_type.into())?;

        let res = self.client
            .get(url)
            .bearer_auth(bearer)
            .send()
            .await?
            .json::<DataResponse>()
            .await?;

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
impl<T: KeyManager + Sync + Send> DigitalOceanClient for DigitalOceanClientImpl<T> {
    async fn list_droplets(&self,
                           per_page: u64,
                           page: u64) -> anyhow::Result<ListDropletsResponse> {
        let mut url = Url::parse(self.config.droplets.url.as_str())?;
        url.query_pairs_mut()
            .append_pair("per_page", per_page.to_string().as_str())
            .append_pair("page", page.to_string().as_str());

        let bearer = self.token_manager.acquire_key(RequestType::Droplets.into())?;

        let res = self.client
            .get(url)
            .bearer_auth(bearer)
            .send()
            .await?
            .json::<ListDropletsResponse>()
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

        let res = self.client
            .get(url)
            .bearer_auth(bearer.as_str())
            .send()
            .await?
            .json::<DataResponse>()
            .await?;

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

    async fn get_file_system_free(&self,
                                  host_id: u64,
                                  start: chrono::DateTime<Utc>,
                                  end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        self.base_metrics_request(
            RequestType::FileSystemFree,
            host_id,
            start,
            end,
        ).await
    }

    async fn get_file_system_size(&self,
                                  host_id: u64,
                                  start: chrono::DateTime<Utc>,
                                  end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        self.base_metrics_request(
            RequestType::FileSystemSize,
            host_id,
            start,
            end,
        ).await
    }


    async fn get_droplet_cached_memory(&self,
                                       host_id: u64,
                                       start: chrono::DateTime<Utc>,
                                       end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        self.base_metrics_request(
            RequestType::CachedMemory,
            host_id,
            start,
            end,
        ).await
    }

    async fn get_droplet_free_memory(&self,
                                     host_id: u64,
                                     start: chrono::DateTime<Utc>,
                                     end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        self.base_metrics_request(
            RequestType::FreeMemory,
            host_id,
            start,
            end,
        ).await
    }

    async fn get_droplet_total_memory(&self,
                                      host_id: u64,
                                      start: chrono::DateTime<Utc>,
                                      end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        self.base_metrics_request(
            RequestType::TotalMemory,
            host_id,
            start,
            end,
        ).await
    }

    async fn get_available_total_memory(&self,
                                        host_id: u64,
                                        start: chrono::DateTime<Utc>,
                                        end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        self.base_metrics_request(
            RequestType::AvailableTotalMemory,
            host_id,
            start,
            end,
        ).await
    }

    async fn get_load(&self,
                      host_id: u64,
                      load_type: LoadType,
                      start: chrono::DateTime<Utc>,
                      end: chrono::DateTime<Utc>) -> anyhow::Result<DataResponse> {
        let request_type = match load_type {
            LoadType::Load1 => RequestType::Load1,
            LoadType::Load5 => RequestType::Load5,
            LoadType::Load15 => RequestType::Load15,
        };

        self.base_metrics_request(
            request_type,
            host_id,
            start,
            end,
        ).await
    }
}