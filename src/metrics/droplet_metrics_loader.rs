use std::sync::Arc;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use prometheus::Opts;
use crate::client::do_client::{ClientLoadType, DigitalOceanClient, FileSystemRequest, MemoryRequest, NetworkDirection, NetworkInterface};
use crate::client::do_json_protocol::{DataResponse, MetricMetaInfo, MetricsResponse};
use crate::config::config_model::{AppSettings, BandwidthType, FilesystemTypes, LoadTypes, MemoryTypes};
use crate::metrics::droplet_store::DropletStore;
use crate::metrics::utils;

#[async_trait]
pub trait DropletMetricsService: Send + Sync {
    async fn load_bandwidth(&self) -> anyhow::Result<()>;
    async fn load_cpu_metrics(&self) -> anyhow::Result<()>;
    async fn load_filesystem_metrics(&self) -> anyhow::Result<()>;
    async fn load_memory_metrics(&self) -> anyhow::Result<()>;
    async fn load_load_metrics(&self) -> anyhow::Result<()>;
}


#[derive(Clone)]
pub struct DropletMetricsServiceImpl {
    client: Arc<dyn DigitalOceanClient>,
    droplet_store: Arc<dyn DropletStore>,
    configs: &'static AppSettings,
    metrics: LoaderMetrics,
}

impl DropletMetricsServiceImpl {
    pub fn new(client: Arc<dyn DigitalOceanClient>,
               droplet_store: Arc<dyn DropletStore>,
               configs: &'static AppSettings,
               registry: prometheus::Registry) -> anyhow::Result<Self> {
        let result = Self {
            client,
            droplet_store,
            configs,
            metrics: LoaderMetrics::new(registry)?,
        };
        Ok(result)
    }
}

#[derive(Clone)]
struct LoaderMetrics {
    droplet_bandwidth: prometheus::GaugeVec,
    droplet_cpu: prometheus::GaugeVec,
    droplet_filesystem: prometheus::GaugeVec,
    droplet_memory: prometheus::GaugeVec,
    droplet_load: prometheus::GaugeVec,
}

impl LoaderMetrics {
    fn new(registry: prometheus::Registry) -> anyhow::Result<Self> {
        let droplet_bandwidth = prometheus::GaugeVec::new(
            Opts::new("droxporter_droplet_bandwidth", "Bandwidth of droplet"),
            &["droplet", "interface", "direction"],
        )?;
        let droplet_cpu = prometheus::GaugeVec::new(
            Opts::new("droxporter_droplet_cpu", "CPU usage of droplet"),
            &["droplet", "mode"],
        )?;
        let droplet_filesystem = prometheus::GaugeVec::new(
            Opts::new("droxporter_droplet_filesystem", "Filesystem usage of droplet"),
            &["droplet", "metric_type", "device", "fstype", "mountpoint"],
        )?;
        let droplet_memory = prometheus::GaugeVec::new(
            Opts::new("droxporter_droplet_memory", "Memory usage of droplet"),
            &["droplet", "metric_type"],
        )?;
        let droplet_load = prometheus::GaugeVec::new(
            Opts::new("droxporter_droplet_load", "Load of droplet"),
            &["droplet", "metric_type"],
        )?;
        registry.register(Box::new(droplet_bandwidth.clone()))?;
        registry.register(Box::new(droplet_cpu.clone()))?;
        registry.register(Box::new(droplet_filesystem.clone()))?;
        registry.register(Box::new(droplet_memory.clone()))?;
        registry.register(Box::new(droplet_load.clone()))?;
        let result = Self {
            droplet_bandwidth,
            droplet_cpu,
            droplet_filesystem,
            droplet_memory,
            droplet_load,
        };
        Ok(result)
    }
}

macro_rules! unwrap_or_return_ok {
    ($block:expr) => {
        match $block {
            None => {return Ok(())},
            Some(x) => x
        }
    }
}

fn extract_last_value(response: DataResponse) -> f64 {
    response.data.result.iter()
        .flat_map(|x| x.values.iter())
        .max_by_key(|x| x.timestamp)
        .and_then(|x| x.value.parse::<f64>().ok())
        .unwrap_or(0f64)
}

fn extract_meta_with_last_values(response: DataResponse) -> Vec<(MetricMetaInfo, f64)> {
    response.data.result.into_iter()
        .map(|x| {
            let last_point = last_point_for_metric(&x);
            let info = x.metric;
            (info, last_point)
        }).collect()
}

fn last_point_for_metric(metrics: &MetricsResponse) -> f64 {
    metrics.values.iter()
        .max_by_key(|x| x.timestamp)
        .and_then(|x| x.value.parse::<f64>().ok())
        .unwrap_or(0f64)
}

fn metrics_read_interval() -> Duration {
    // It seems that DO has a 10..15 second interval between points, so I think an interval of 1 minute is reasonable.
    Duration::minutes(1)
}

// a lot of boilerplate. but I don't think it would be changing too often
#[async_trait]
impl DropletMetricsService for DropletMetricsServiceImpl {
    async fn load_bandwidth(&self) -> anyhow::Result<()> {
        let bandwidth = unwrap_or_return_ok!(self.configs.metrics.bandwidth.as_ref());

        let enable_private_in = bandwidth.types.contains(&BandwidthType::PrivateInbound);
        let enable_private_out = bandwidth.types.contains(&BandwidthType::PrivateOutbound);
        let enable_public_in = bandwidth.types.contains(&BandwidthType::PublicInbound);
        let enable_public_out = bandwidth.types.contains(&BandwidthType::PublicOutbound);

        let metric_types: Vec<_> = [
            (enable_private_in, NetworkInterface::Private, NetworkDirection::Inbound),
            (enable_private_out, NetworkInterface::Private, NetworkDirection::Outbound),
            (enable_public_in, NetworkInterface::Public, NetworkDirection::Inbound),
            (enable_public_out, NetworkInterface::Public, NetworkDirection::Outbound),
        ].iter()
            .filter(|(enabled, _, _)| *enabled)
            .map(|(_, interface, dir)| (*interface, *dir))
            .collect();

        let interval_end = Utc::now();
        let interval_start = interval_end - metrics_read_interval();

        let droplets = self.droplet_store.list_droplets();
        for droplet in droplets.iter() {
            for (interface, dir) in &metric_types {
                let res = self.client
                    .get_bandwidth(
                        droplet.id,
                        *interface,
                        *dir,
                        interval_start,
                        interval_end
                    ).await?;
                let value = extract_last_value(res);
                let interface = match interface {
                    NetworkInterface::Public => "public",
                    NetworkInterface::Private => "private"
                };
                let direction = match dir {
                    NetworkDirection::Inbound => "inbound",
                    NetworkDirection::Outbound => "outbound"
                };

                self.metrics.droplet_bandwidth
                    .with_label_values(&[
                        droplet.name.as_str(),
                        interface,
                        direction,
                    ]).set(value);
            }
        }

        let droplets = self.droplet_store.list_droplets();
        let droplets_names: ahash::HashSet<_> = droplets
            .iter()
            .map(|x| x.name.as_str())
            .collect();
        utils::remove_old_droplets(&self.metrics.droplet_bandwidth, &droplets_names);

        Ok(())
    }

    async fn load_cpu_metrics(&self) -> anyhow::Result<()> {
        let interval_end = Utc::now();
        let interval_start = interval_end - metrics_read_interval();

        for droplet in self.droplet_store.list_droplets().iter() {
            let res = self.client
                .get_cpu(
                    droplet.id,
                    interval_start,
                    interval_end
                ).await?;
            for (meta, value) in extract_meta_with_last_values(res) {
                let mode = meta.mode.as_ref().map(String::as_str).unwrap_or("unknown");
                self.metrics.droplet_cpu
                    .with_label_values(&[
                        droplet.name.as_str(),
                        mode
                    ]).set(value);
            }
        }

        let droplets = self.droplet_store.list_droplets();
        let droplets_names: ahash::HashSet<_> = droplets
            .iter()
            .map(|x| x.name.as_str())
            .collect();
        utils::remove_old_droplets(&self.metrics.droplet_cpu, &droplets_names);

        Ok(())
    }

    async fn load_filesystem_metrics(&self) -> anyhow::Result<()> {
        let filesystem = unwrap_or_return_ok!(self.configs.metrics.filesystem.as_ref());

        let enable_free = filesystem.types.contains(&FilesystemTypes::Free);
        let enable_size = filesystem.types.contains(&FilesystemTypes::Size);

        let filesystem_types: Vec<_> = [
            (enable_free, FileSystemRequest::Free),
            (enable_size, FileSystemRequest::Size),
        ].iter().filter(|(enabled, _)| *enabled)
            .map(|(_, client_type)| *client_type)
            .collect();

        let interval_end = Utc::now();
        let interval_start = interval_end - metrics_read_interval();

        for droplet in self.droplet_store.list_droplets().iter() {
            for metrics_type in &filesystem_types {
                let res = self.client
                    .get_file_system(
                        droplet.id,
                        *metrics_type,
                        interval_start,
                        interval_end
                    ).await?;

                let fs_metrics_type_str = match metrics_type {
                    FileSystemRequest::Free => "free",
                    FileSystemRequest::Size => "size"
                };
                for (meta, value) in extract_meta_with_last_values(res) {
                    let device = meta.device.as_ref().map(String::as_str).unwrap_or("unknown");
                    let fstype = meta.fstype.as_ref().map(String::as_str).unwrap_or("unknown");
                    let mountpoint = meta.mountpoint.as_ref().map(String::as_str).unwrap_or("unknown");

                    self.metrics.droplet_filesystem
                        .with_label_values(&[
                            droplet.name.as_str(),
                            fs_metrics_type_str,
                            device,
                            fstype,
                            mountpoint
                        ]).set(value);
                }
            }
        }

        let droplets = self.droplet_store.list_droplets();
        let droplets_names: ahash::HashSet<_> = droplets
            .iter()
            .map(|x| x.name.as_str())
            .collect();
        utils::remove_old_droplets(&self.metrics.droplet_filesystem, &droplets_names);

        Ok(())
    }

    async fn load_memory_metrics(&self) -> anyhow::Result<()> {
        let memory = unwrap_or_return_ok!(self.configs.metrics.memory.as_ref());

        let enable_free = memory.types.contains(&MemoryTypes::Free);
        let enable_available = memory.types.contains(&MemoryTypes::Available);
        let enable_cached = memory.types.contains(&MemoryTypes::Cached);
        let enable_total = memory.types.contains(&MemoryTypes::Total);

        let memory_types: Vec<_> = [
            (enable_free, MemoryRequest::FreeMemory),
            (enable_available, MemoryRequest::AvailableTotalMemory),
            (enable_cached, MemoryRequest::CachedMemory),
            (enable_total, MemoryRequest::TotalMemory),
        ].iter().filter(|(enabled, _)| *enabled)
            .map(|(_, client_type)| *client_type)
            .collect();

        let interval_end = Utc::now();
        let interval_start = interval_end - Duration::minutes(30);

        for droplet in self.droplet_store.list_droplets().iter() {
            for memory_type in &memory_types {
                let res = self.client
                    .get_droplet_memory(
                        droplet.id,
                        *memory_type,
                        interval_start,
                        interval_end,
                    ).await?;
                let value = extract_last_value(res);

                let memory_type_str = match memory_type {
                    MemoryRequest::CachedMemory => "cached",
                    MemoryRequest::FreeMemory => "free",
                    MemoryRequest::TotalMemory => "total",
                    MemoryRequest::AvailableTotalMemory => "available",
                };

                self.metrics.droplet_memory
                    .with_label_values(&[
                        droplet.name.as_str(),
                        memory_type_str,
                    ]).set(value);
            }
        }

        let droplets = self.droplet_store.list_droplets();
        let droplets_names: ahash::HashSet<_> = droplets
            .iter()
            .map(|x| x.name.as_str())
            .collect();
        utils::remove_old_droplets(&self.metrics.droplet_memory, &droplets_names);

        Ok(())
    }

    async fn load_load_metrics(&self) -> anyhow::Result<()> {
        let load = unwrap_or_return_ok!(self.configs.metrics.load.as_ref());

        let enable_load1 = load.types.contains(&LoadTypes::Load1);
        let enable_load5 = load.types.contains(&LoadTypes::Load5);
        let enable_load15 = load.types.contains(&LoadTypes::Load15);

        let load_types: Vec<_> = [
            (enable_load1, ClientLoadType::Load1),
            (enable_load5, ClientLoadType::Load5),
            (enable_load15, ClientLoadType::Load15),
        ].iter().filter(|(enabled, _)| *enabled)
            .map(|(_, client_type)| *client_type)
            .collect();

        let interval_end = Utc::now();
        let interval_start = interval_end - Duration::minutes(30);

        for droplet in self.droplet_store.list_droplets().iter() {
            for load_type in &load_types {
                let res = self.client
                    .get_load(
                        droplet.id,
                        *load_type,
                        interval_start,
                        interval_end
                    )
                    .await?;
                let value = extract_last_value(res);

                let load_type_str = match load_type {
                    ClientLoadType::Load1 => "load_1",
                    ClientLoadType::Load5 => "load_5",
                    ClientLoadType::Load15 => "load_15"
                };

                self.metrics.droplet_load
                    .with_label_values(&[
                        droplet.name.as_str(),
                        load_type_str,
                    ]).set(value);
            }
        }

        let droplets = self.droplet_store.list_droplets();
        let droplets_names: ahash::HashSet<_> = droplets
            .iter()
            .map(|x| x.name.as_str())
            .collect();
        utils::remove_old_droplets(&self.metrics.droplet_load, &droplets_names);

        Ok(())
    }
}