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