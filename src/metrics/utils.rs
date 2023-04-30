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

// Personally, I prefer Summaries because they are more accurate, but in Rust I have no choice =(
pub const DROXPORTER_DEFAULT_BUCKETS: &[f64; 16] = &[
    0.001, 0.004, 0.008, 0.016, 0.032, 0.064, 0.128, 0.256, 0.512, 1.024, 2.048, 8.192, 16.384, 32.768, 131.072, 262.144
];
