use serde::Deserialize;

pub type Key = String;

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AppSettings {
    #[serde(default)]
    pub default_keys: Vec<Key>,
    pub droplets: DropletSettings,
    pub metrics: MetricsConfig,
    pub agent_metrics: AgentMetricsConfigs,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct MetricsConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,

    pub bandwidth: Option<BandwidthSettings>,
    pub cpu: Option<CpuSettings>,
    pub filesystem: Option<FilesystemSettings>,
    pub memory: Option<MemorySettings>,
    pub load: Option<LoadSettings>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AgentMetricsConfigs {
    #[serde(default)]
    pub metrics: Vec<AgentMetricsType>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "duration_10_seconds")]
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
}

#[derive(Deserialize, Clone, PartialEq, Eq)]
pub enum AgentMetricsType {
    #[serde(rename = "memory")]
    Memory,
    #[serde(rename = "cpu")]
    Cpu,
    #[serde(rename = "limits")]
    Limits,
    #[serde(rename = "requests")]
    Requests,
    #[serde(rename = "jobs")]
    Jobs,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct DropletSettings {
    #[serde(default)]
    pub keys: Vec<Key>,
    #[serde(default = "default_droplets_url")]
    pub url: String,
    #[serde(default = "duration_120_seconds")]
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
    #[serde(default)]
    pub metrics: Vec<DropletMetricsTypes>,
}

#[derive(Deserialize, Clone, PartialEq, Eq)]
pub enum DropletMetricsTypes {
    #[serde(rename = "memory")]
    Memory,
    #[serde(rename = "vcpu")]
    VCpu,
    #[serde(rename = "disk")]
    Disk,
    #[serde(rename = "status")]
    Status,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct BandwidthSettings {
    #[serde(default = "default_bandwidth_types")]
    pub types: Vec<BandwidthType>,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default = "duration_10_seconds")]
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Deserialize, Clone, PartialEq, Eq)]
pub enum BandwidthType {
    #[serde(rename = "private_inbound")]
    PrivateInbound,
    #[serde(rename = "private_outbound")]
    PrivateOutbound,
    #[serde(rename = "public_inbound")]
    PublicInbound,
    #[serde(rename = "public_outbound")]
    PublicOutbound,
}

fn default_bandwidth_types() -> Vec<BandwidthType> {
    vec![
        BandwidthType::PrivateInbound,
        BandwidthType::PrivateOutbound,
        BandwidthType::PublicInbound,
        BandwidthType::PublicOutbound,
    ]
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct CpuSettings {
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default = "duration_10_seconds")]
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct FilesystemSettings {
    #[serde(default)]
    pub types: Vec<FilesystemTypes>,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default = "duration_120_seconds")]
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Deserialize, Clone, Eq, PartialEq)]
pub enum FilesystemTypes {
    #[serde(rename = "free")]
    Free,
    #[serde(rename = "size")]
    Size,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct MemorySettings {
    #[serde(default)]
    pub types: Vec<MemoryTypes>,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default = "duration_10_seconds")]
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Deserialize, Clone, Eq, PartialEq)]
pub enum MemoryTypes {
    #[serde(rename = "cached")]
    Cached,
    #[serde(rename = "free")]
    Free,
    #[serde(rename = "total")]
    Total,
    #[serde(rename = "available")]
    Available,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct LoadSettings {
    #[serde(default)]
    pub types: Vec<LoadTypes>,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default = "duration_10_seconds")]
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Deserialize, Clone, Eq, PartialEq)]
pub enum LoadTypes {
    #[serde(rename = "load_1")]
    Load1,
    #[serde(rename = "load_5")]
    Load5,
    #[serde(rename = "load_15")]
    Load15,
}


// defaults for serde

fn duration_10_seconds() -> std::time::Duration {
    std::time::Duration::from_secs(10)
}

fn duration_60_seconds() -> std::time::Duration {
    std::time::Duration::from_secs(60)
}

fn duration_120_seconds() -> std::time::Duration {
    std::time::Duration::from_secs(120)
}

fn default_base_url() -> String {
    "https://api.digitalocean.com/v2/monitoring/metrics/droplet".into()
}

fn default_droplets_url() -> String {
    "https://api.digitalocean.com/v2/droplets".into()
}

fn default_true() -> bool {
    true
}