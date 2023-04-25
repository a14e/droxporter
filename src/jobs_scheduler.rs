use crate::config::MetricsManagerConfigs;
use crate::do_client::DigitalOceanClient;

struct MetricsScheduler<DoClinent: DigitalOceanClient> {
    client: DoClinent,
    configs: MetricsManagerConfigs
}














