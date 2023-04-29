use ahash::HashSet;
use prometheus::core::Collector;

pub fn remove_old_droplets(gauge: &prometheus::GaugeVec,
                           valid_droplets: &HashSet<&str>) {
    let labels_to_delete = gauge.collect()
        .into_iter()
        .filter(|m| {
            m.get_metric()
                .iter()
                .flat_map(|x| x.get_label().iter())
                .find(|label| label.get_name() == "droplet")
                .iter()
                .all(|label| !valid_droplets.contains(label.get_value()))
        });

    for m in labels_to_delete {
        let labels: std::collections::HashMap<_, _> = m.get_metric()
            .iter()
            .flat_map(|x| x.get_label())
            .map(|l| (l.get_name(), l.get_value()))
            .collect();

        let _ = gauge.remove(&labels);
    }
}


pub const DROXPORTER_DEFAULT_BUCKETS: &[f64; 19] = &[
    0.001, 0.002, 0.004, 0.006, 0.008, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 50.0, 100.0, 200.0, 300.0
];
