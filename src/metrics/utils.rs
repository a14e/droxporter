use ahash::HashSet;
use prometheus::core::Collector;

pub fn remove_old_droplets(gauge: &prometheus::GaugeVec, valid_droplets: &HashSet<&str>) {
    let labels_to_delete: Vec<_> = gauge
        .collect()
        .iter()
        .flat_map(|m| m.get_metric().to_vec())
        .filter(|m| {
            m.get_label()
                .iter()
                .find(|label| label.get_name() == "droplet")
                .iter()
                .all(|label| !valid_droplets.contains(label.get_value()))
        })
        .collect();

    for m in labels_to_delete {
        let labels: std::collections::HashMap<_, _> = m
            .get_label()
            .iter()
            .map(|l| (l.get_name(), l.get_value()))
            .collect();

        let _ = gauge.remove(&labels);
    }
}

pub fn remove_old_apps_for_gauge_metric(gauge: &prometheus::GaugeVec, valid_apps: &HashSet<&str>) {
    let labels_to_delete: Vec<_> = gauge
        .collect()
        .iter()
        .flat_map(|m| m.get_metric().to_vec())
        .filter(|m| {
            m.get_label()
                .iter()
                .find(|label| label.get_name() == "app")
                .iter()
                .all(|label| !valid_apps.contains(label.get_value()))
        })
        .collect();

    for m in labels_to_delete {
        let labels: std::collections::HashMap<_, _> = m
            .get_label()
            .iter()
            .map(|l| (l.get_name(), l.get_value()))
            .collect();

        let _ = gauge.remove(&labels);
    }
}

pub fn remove_old_apps_for_counter_metric(
    counter: &prometheus::CounterVec,
    valid_apps: &HashSet<&str>,
) {
    let labels_to_delete: Vec<_> = counter
        .collect()
        .iter()
        .flat_map(|m| m.get_metric().to_vec())
        .filter(|m| {
            m.get_label()
                .iter()
                .find(|label| label.get_name() == "app")
                .iter()
                .all(|label| !valid_apps.contains(label.get_value()))
        })
        .collect();

    for m in labels_to_delete {
        let labels: std::collections::HashMap<_, _> = m
            .get_label()
            .iter()
            .map(|l| (l.get_name(), l.get_value()))
            .collect();

        let _ = counter.remove(&labels);
    }
}

// Personally, I prefer Summaries because they are more accurate, but in Rust I have no choice =(
pub const DROXPORTER_DEFAULT_BUCKETS: &[f64; 16] = &[
    0.001, 0.004, 0.008, 0.016, 0.032, 0.064, 0.128, 0.256, 0.512, 1.024, 2.048, 8.192, 16.384,
    32.768, 131.072, 262.144,
];

#[cfg(test)]
mod tests {
    use super::*;
    use ahash::HashSetExt;
    use prometheus::{CounterVec, GaugeVec, Opts, Registry};

    fn create_test_gauge_vec(name: &str, label_names: &[&str]) -> GaugeVec {
        let gauge = GaugeVec::new(Opts::new(name, "test gauge"), label_names).unwrap();
        let registry = Registry::new();
        registry.register(Box::new(gauge.clone())).unwrap();
        gauge
    }

    fn create_test_counter_vec(name: &str, label_names: &[&str]) -> CounterVec {
        let counter = CounterVec::new(Opts::new(name, "test counter"), label_names).unwrap();
        let registry = Registry::new();
        registry.register(Box::new(counter.clone())).unwrap();
        counter
    }

    #[test]
    fn test_remove_old_droplets() {
        let gauge = create_test_gauge_vec("test_droplets", &["droplet"]);

        // Create metrics for droplets A, B, C
        gauge.with_label_values(&["droplet-A"]).set(10.0);
        gauge.with_label_values(&["droplet-B"]).set(20.0);
        gauge.with_label_values(&["droplet-C"]).set(30.0);

        // Now only droplets A and C are valid
        let valid_droplets: HashSet<&str> = ["droplet-A", "droplet-C"].iter().cloned().collect();

        remove_old_droplets(&gauge, &valid_droplets);

        // Check that droplet-B was removed
        let metric_families = gauge.collect();
        let metrics: Vec<_> = metric_families
            .iter()
            .flat_map(|m| m.get_metric().to_vec())
            .collect();

        assert_eq!(metrics.len(), 2);

        let droplet_names: HashSet<_> = metrics
            .iter()
            .filter_map(|m| m.get_label().iter().find(|l| l.get_name() == "droplet"))
            .map(|l| l.get_value())
            .collect();

        assert!(droplet_names.contains(&"droplet-A"));
        assert!(droplet_names.contains(&"droplet-C"));
        assert!(!droplet_names.contains(&"droplet-B"));
    }

    #[test]
    fn test_remove_old_apps_gauge() {
        let gauge = create_test_gauge_vec("test_apps", &["app", "component"]);

        // Create metrics for apps
        gauge.with_label_values(&["app-1", "web"]).set(1.0);
        gauge.with_label_values(&["app-2", "api"]).set(2.0);
        gauge.with_label_values(&["app-3", "worker"]).set(3.0);

        // Only app-1 and app-3 are valid
        let valid_apps: HashSet<&str> = ["app-1", "app-3"].iter().cloned().collect();

        remove_old_apps_for_gauge_metric(&gauge, &valid_apps);

        // Check that app-2 was removed
        let metric_families = gauge.collect();
        let metrics: Vec<_> = metric_families
            .iter()
            .flat_map(|m| m.get_metric().to_vec())
            .collect();

        assert_eq!(metrics.len(), 2);

        let app_names: HashSet<_> = metrics
            .iter()
            .filter_map(|m| m.get_label().iter().find(|l| l.get_name() == "app"))
            .map(|l| l.get_value())
            .collect();

        assert!(app_names.contains(&"app-1"));
        assert!(app_names.contains(&"app-3"));
        assert!(!app_names.contains(&"app-2"));
    }

    #[test]
    fn test_remove_old_apps_counter() {
        let counter = create_test_counter_vec("test_counters", &["app", "instance"]);

        // Create metrics for apps
        counter.with_label_values(&["app-1", "instance-1"]).inc();
        counter.with_label_values(&["app-2", "instance-1"]).inc();
        counter.with_label_values(&["app-3", "instance-1"]).inc();

        // Only app-1 and app-3 are valid
        let valid_apps: HashSet<&str> = ["app-1", "app-3"].iter().cloned().collect();

        remove_old_apps_for_counter_metric(&counter, &valid_apps);

        // Check that app-2 was removed
        let metric_families = counter.collect();
        let metrics: Vec<_> = metric_families
            .iter()
            .flat_map(|m| m.get_metric().to_vec())
            .collect();

        assert_eq!(metrics.len(), 2);

        let app_names: HashSet<_> = metrics
            .iter()
            .filter_map(|m| m.get_label().iter().find(|l| l.get_name() == "app"))
            .map(|l| l.get_value())
            .collect();

        assert!(app_names.contains(&"app-1"));
        assert!(app_names.contains(&"app-3"));
        assert!(!app_names.contains(&"app-2"));
    }

    #[test]
    fn test_remove_old_droplets_empty_valid_set() {
        let gauge = create_test_gauge_vec("test_droplets", &["droplet"]);

        // Create metrics for droplets
        gauge.with_label_values(&["droplet-A"]).set(10.0);
        gauge.with_label_values(&["droplet-B"]).set(20.0);

        // No valid droplets
        let valid_droplets: HashSet<&str> = HashSet::new();

        remove_old_droplets(&gauge, &valid_droplets);

        // All droplets should be removed
        let metric_families = gauge.collect();
        let metrics: Vec<_> = metric_families
            .iter()
            .flat_map(|m| m.get_metric().to_vec())
            .collect();

        assert_eq!(metrics.len(), 0);
    }

    #[test]
    fn test_remove_old_no_metrics() {
        let gauge = create_test_gauge_vec("test_empty", &["droplet"]);

        // No metrics created
        let valid_droplets: HashSet<&str> = ["droplet-A"].iter().cloned().collect();

        // Should not panic
        remove_old_droplets(&gauge, &valid_droplets);

        let metric_families = gauge.collect();
        let metrics: Vec<_> = metric_families
            .iter()
            .flat_map(|m| m.get_metric().to_vec())
            .collect();

        assert_eq!(metrics.len(), 0);
    }
}
