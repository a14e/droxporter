use std::collections::HashMap;
use serde::Deserialize;

pub type Key = String;

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AppSettings {
    #[serde(default)]
    pub default_keys: Vec<Key>,
    #[serde(default)]
    pub droplets: DropletSettings,
    #[serde(default)]
    pub apps: AppPlatformSettings,
    #[serde(default, alias = "metrics")]
    pub droplet_metrics: DropletMetricsConfig,
    #[serde(default)]
    pub app_metrics: AppMetricsConfig,
    #[serde(default)]
    pub exporter_metrics: ExporterMetricsConfigs,
    #[serde(default)]
    pub endpoint: EndpointConfig,
    #[serde(default)]
    pub custom: CustomSettings
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct CustomSettings {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct EndpointConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_host")]
    pub host: String,
    pub auth: Option<AuthSettings>,
    pub ssl: Option<SslSettings>,
}

fn default_port() -> u16 {
    8888
}

fn default_host() -> String {
    "0.0.0.0".into()
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AuthSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_login")]
    pub login: String,
    #[serde(default = "default_password")]
    pub password: String,
}

fn default_login() -> String {
    "login".into()
}

fn default_password() -> String {
    "password".into()
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SslSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ssl_cert")]
    pub root_cert_path: String,
    #[serde(default = "default_ssl_key")]
    pub key_path: String,
}

fn default_ssl_cert() -> String {
    "./cert.pem".into()
}

fn default_ssl_key() -> String {
    "./key.pem".into()
}


#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct DropletMetricsConfig {
    #[serde(default = "default_droplet_metrics_base_url")]
    pub base_url: String,

    pub bandwidth: Option<BandwidthSettings>,
    pub cpu: Option<CpuSettings>,
    pub filesystem: Option<FilesystemSettings>,
    pub memory: Option<MemorySettings>,
    pub load: Option<LoadSettings>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AppMetricsConfig {
    #[serde(default = "default_app_metrics_base_url")]
    pub base_url: String,

    pub cpu_percentage: Option<AppCpuPercentageSettings>,
    pub memory_percentage: Option<AppMemoryPercentageSettings>,
    pub restart_count: Option<AppRestartCountSettings>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ExporterMetricsConfigs {
    #[serde(default)]
    pub metrics: Vec<AgentMetricsType>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "duration_5_seconds")]
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
    #[serde(default = "duration_1_hour")]
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
    #[serde(default)]
    pub metrics: Vec<DropletMetricsTypes>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AppPlatformSettings {
    #[serde(default)]
    pub keys: Vec<Key>,
    #[serde(default = "default_apps_url")]
    pub url: String,
    #[serde(default = "duration_1_hour")]
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
    #[serde(default)]
    pub metrics: Vec<AppMetricsTypes>,
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

#[derive(Deserialize, Clone, PartialEq, Eq)]
pub enum AppMetricsTypes {
    #[serde(rename = "active_deployment_phase")]
    ActiveDeploymentPhase,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct BandwidthSettings {
    #[serde(default)]
    pub types: Vec<BandwidthType>,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default = "duration_60_seconds")]
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

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct CpuSettings {
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default = "duration_45_seconds")]
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
    #[serde(default = "duration_120_seconds")]
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
    #[serde(default = "duration_120_seconds")]
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

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AppCpuPercentageSettings {
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default = "duration_60_seconds")]
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AppMemoryPercentageSettings {
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default = "duration_60_seconds")]
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AppRestartCountSettings {
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default = "duration_60_seconds")]
    #[serde(with = "humantime_serde")]
    pub interval: std::time::Duration,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn duration_1_hour() -> std::time::Duration {
    std::time::Duration::from_secs(60 * 60)
}

fn duration_5_seconds() -> std::time::Duration {
    std::time::Duration::from_secs(60)
}

fn duration_60_seconds() -> std::time::Duration {
    std::time::Duration::from_secs(60)
}

fn duration_45_seconds() -> std::time::Duration {
    std::time::Duration::from_secs(45)
}

fn duration_120_seconds() -> std::time::Duration {
    std::time::Duration::from_secs(120)
}

fn default_droplet_metrics_base_url() -> String {
    "https://api.digitalocean.com/v2/monitoring/metrics/droplet".into()
}

fn default_app_metrics_base_url() -> String {
    "https://api.digitalocean.com/v2/monitoring/metrics/apps".into()
}

fn default_droplets_url() -> String {
    "https://api.digitalocean.com/v2/droplets".into()
}

fn default_apps_url() -> String {
    "https://api.digitalocean.com/v2/apps".into()
}

fn default_true() -> bool {
    true
}
