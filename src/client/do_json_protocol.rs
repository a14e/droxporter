use std::fmt;
use serde::{Deserialize};
use serde::de::{SeqAccess, Visitor};

#[derive(Deserialize, PartialEq, Debug)]
pub struct ListDropletsResponse {
    pub droplets: Vec<DropletResponse>,
}


#[derive(Deserialize, PartialEq, Debug)]
pub struct DropletResponse {
    pub id: u64,
    pub name: String,
    pub memory: u64,
    pub vcpus: u64,
    pub disk: u64,
    pub locked: bool,
    pub status: String,
}


#[derive(Deserialize, PartialEq, Debug)]
pub struct DataResponse {
    pub status: String,
    pub data: DataResult,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct DataResult {
    pub result: Vec<MetricsResponse>,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct MetricsResponse {
    pub metric: MetricMetaInfo,
    #[serde(deserialize_with = "deserialize_points")]
    pub values: Vec<MetricPoint>,
}

// Of course, I could use generics here, but I think it would be much harder to work with and understand.
// So, I prefer to create a bit of chaos =)
// Also, if Digital Ocean ever decides to change the protocol, everything would continue to work, just with unknown labels.
#[derive(Deserialize, PartialEq, Debug, Default)]
pub struct MetricMetaInfo {
    pub host_id: String,

    // for cpu
    pub mode: Option<String>,

    // for filesystem_free / filesystem_size
    pub device: Option<String>,
    pub fstype: Option<String>,
    pub mountpoint: Option<String>
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct MetricMetaDefault {
    pub host_id: String
}

#[derive(PartialEq, Debug)]
pub struct MetricPoint {
    pub timestamp: u64,
    pub value: String,
}

fn deserialize_points<'de, D>(deserializer: D) -> Result<Vec<MetricPoint>, D::Error>
    where
        D: serde::Deserializer<'de>,
{
    pub struct ValuesVisitor;

    impl<'de> Visitor<'de> for ValuesVisitor {
        type Value = Vec<MetricPoint>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a sequence of arrays with two elements each")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Vec<MetricPoint>, A::Error>
            where
                A: SeqAccess<'de>,
        {
            let mut values = Vec::new();

            while let Some((timestamp, value)) = seq.next_element::<(u64, String)>()? {
                values.push(MetricPoint { timestamp, value });
            }

            Ok(values)
        }
    }

    deserializer.deserialize_seq(ValuesVisitor)
}


#[cfg(test)]
mod deserialize_test {
    use crate::client::do_json_protocol::{DataResponse, DataResult, DropletResponse, ListDropletsResponse, MetricMetaInfo, MetricPoint, MetricsResponse};

    #[test]
    fn deserialize_droplets() {
        let json_data = r#"{"droplets":[{"id":336239184,"name":"traefik-zitadel-1","memory":2048,"vcpus":1,"disk":50,"locked":false,"status":"active","kernel":null,"created_at":"2023-01-17T17:12:08Z","features":["monitoring","droplet_agent","private_networking"],"backup_ids":[],"next_backup_window":null,"snapshot_ids":[],"image":{"id":119383150,"name":"22.10 x64","distribution":"Ubuntu","slug":"ubuntu-22-10-x64","public":true,"regions":["nyc3","nyc1","sfo1","nyc2","ams2","sgp1","lon1","ams3","fra1","tor1","sfo2","blr1","sfo3","syd1"],"created_at":"2022-10-22T19:58:00Z","min_disk_size":7,"type":"base","size_gigabytes":0.74,"description":"Ubuntu 22.10 (LTS) x64","tags":[],"status":"available"},"volume_ids":[],"size":{"slug":"s-1vcpu-2gb","memory":2048,"vcpus":1,"disk":50,"transfer":2.0,"price_monthly":12.0,"price_hourly":0.01786,"regions":["ams3","blr1","fra1","lon1","nyc1","nyc3","sfo3","sgp1","syd1","tor1"],"available":true,"description":"Basic"},"size_slug":"s-1vcpu-2gb","networks":{"v4":[{"ip_address":"164.90.185.107","netmask":"255.255.240.0","gateway":"164.90.176.1","type":"public"},{"ip_address":"10.114.0.3","netmask":"255.255.240.0","gateway":"10.114.0.1","type":"private"}],"v6":[]},"region":{"name":"Frankfurt 1","slug":"fra1","features":["backups","ipv6","metadata","install_agent","storage","image_transfer"],"available":true,"sizes":["s-1vcpu-512mb-10gb","s-1vcpu-1gb","s-1vcpu-1gb-amd","s-1vcpu-1gb-intel","s-1vcpu-2gb","s-1vcpu-2gb-amd","s-1vcpu-2gb-intel","s-2vcpu-2gb","s-2vcpu-2gb-amd","s-2vcpu-2gb-intel","s-2vcpu-4gb","s-2vcpu-4gb-amd","s-2vcpu-4gb-intel","c-2","c2-2vcpu-4gb","s-4vcpu-8gb","s-4vcpu-8gb-amd","s-4vcpu-8gb-intel","g-2vcpu-8gb","gd-2vcpu-8gb","m-2vcpu-16gb","c-4","c2-4vcpu-8gb","s-8vcpu-16gb","m3-2vcpu-16gb","c-4-intel","s-8vcpu-16gb-amd","s-8vcpu-16gb-intel","c2-4vcpu-8gb-intel","g-4vcpu-16gb","so-2vcpu-16gb","m6-2vcpu-16gb","gd-4vcpu-16gb","so1_5-2vcpu-16gb","m-4vcpu-32gb","c-8","c2-8vcpu-16gb","m3-4vcpu-32gb","c-8-intel","c2-8vcpu-16gb-intel","g-8vcpu-32gb","so-4vcpu-32gb","m6-4vcpu-32gb","gd-8vcpu-32gb","so1_5-4vcpu-32gb","m-8vcpu-64gb","c-16","c2-16vcpu-32gb","m3-8vcpu-64gb","c-16-intel","c2-16vcpu-32gb-intel","g-16vcpu-64gb","so-8vcpu-64gb","m6-8vcpu-64gb","gd-16vcpu-64gb","so1_5-8vcpu-64gb","m-16vcpu-128gb","c-32","c2-32vcpu-64gb","m3-16vcpu-128gb","c-32-intel","c2-32vcpu-64gb-intel","m-24vcpu-192gb","g-32vcpu-128gb","so-16vcpu-128gb","m6-16vcpu-128gb","gd-32vcpu-128gb","m3-24vcpu-192gb","g-40vcpu-160gb","so1_5-16vcpu-128gb","c-48-intel","m-32vcpu-256gb","gd-40vcpu-160gb","c2-48vcpu-96gb-intel","so-24vcpu-192gb","m6-24vcpu-192gb","m3-32vcpu-256gb","so1_5-24vcpu-192gb","so-32vcpu-256gb","m6-32vcpu-256gb","so1_5-32vcpu-256gb"]},"tags":[],"vpc_uuid":"addcb62f-5973-465d-964c-4ffcac4f8b52"}],"links":{"pages":{"first":"https://api.digitalocean.com/v2/droplets?page=1\u0026per_page=1","prev":"https://api.digitalocean.com/v2/droplets?page=1\u0026per_page=1","next":"https://api.digitalocean.com/v2/droplets?page=3\u0026per_page=1","last":"https://api.digitalocean.com/v2/droplets?page=3\u0026per_page=1"}},"meta":{"total":3}}"#;
        let deserialized_data: ListDropletsResponse = serde_json::from_str(json_data).unwrap();
        let expected_result = ListDropletsResponse {
            droplets: vec![
                DropletResponse {
                    id: 336239184,
                    name: "traefik-zitadel-1".to_string(),
                    memory: 2048,
                    vcpus: 1,
                    disk: 50,
                    locked: false,
                    status: "active".to_string(),
                }
            ]
        };

        assert_eq!(deserialized_data, expected_result)
    }

    #[test]
    fn deserialize_metrics() {
        let json_data = r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"direction":"inbound","host_id":"335943309","interface":"public"},"values":[[1682246520,"0.00011012000000000001"],[1682246760,"0.00025643564356435644"]]}]}}"#;
        let deserialized_data: DataResponse = serde_json::from_str(json_data).unwrap();
        let expected_result = DataResponse {
            status: "success".into(),
            data: DataResult {
                result: vec![
                    MetricsResponse {
                        metric: MetricMetaInfo {
                            host_id: "335943309".into(),
                            ..Default::default()
                        },
                        values: vec![
                            MetricPoint { timestamp: 1682246520, value: "0.00011012000000000001".into() },
                            MetricPoint { timestamp: 1682246760, value: "0.00025643564356435644".into() },
                        ]
                    }
                ]
            },
        };

        assert_eq!(deserialized_data, expected_result)
    }
}