use std::sync::Arc;
use parking_lot::Mutex;
use prometheus::{Gauge, Registry};
use sysinfo::{ProcessExt, System, SystemExt};
use crate::config::config_model::{AgentMetricsType, AppSettings};

pub trait AgentMetricsService: Send + Sync {
    fn load_agent_metrics(&self) -> anyhow::Result<()>;
}


#[derive(Clone)]
pub struct AgentMetricsImpl {
    config: &'static AppSettings,
    system: Arc<Mutex<System>>,
    cpu: Gauge,
    memory: Gauge,
    start_time: Gauge,
}

impl AgentMetricsImpl {
    pub fn new(config: &'static AppSettings,
               registry: Registry) -> Self {
        let system = System::new();
        let cpu = Gauge::new("droxporter_self_cpu_usage_percents", "CPU usage of DO Loading agent").unwrap();
        let memory = Gauge::new("droxporter_self_memory_usage", "CPU usage of DO Loading agent").unwrap();
        let start_time = Gauge::new("droxporter_self_start_time_seconds", "Start time  (in seconds) from epoch of DO Loading agent").unwrap();
        registry.register(Box::new(cpu.clone())).unwrap();
        registry.register(Box::new(memory.clone())).unwrap();
        registry.register(Box::new(start_time.clone())).unwrap();
        Self {
            config,
            system: Arc::new(Mutex::new(system)),
            cpu,
            memory,
            start_time
        }
    }
}


impl AgentMetricsService for AgentMetricsImpl {
    fn load_agent_metrics(&self) -> anyhow::Result<()> {
        let cpu_metrics_enabled = self.config.exporter_metrics.enabled && {
            self.config.exporter_metrics.metrics.contains(&AgentMetricsType::Cpu)
        };
        let memory_metrics_enabled = self.config.exporter_metrics.enabled && {
            self.config.exporter_metrics.metrics.contains(&AgentMetricsType::Memory)
        };

        let mut system = self.system.lock();
        system.refresh_all();

        let pid = sysinfo::get_current_pid()
            .map_err(|s| anyhow::Error::msg(s))?;
        let process = system.process(pid)
            .ok_or(anyhow::Error::msg("Process not found"))?;

        if cpu_metrics_enabled {
            self.cpu.set(process.cpu_usage() as f64);
        }
        if memory_metrics_enabled {
            self.memory.set((process.memory()) as f64);
        }
        self.start_time.set(process.start_time()as f64);
        Ok(())
    }
}