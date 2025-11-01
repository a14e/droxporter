use serde::Deserialize;
use serde::de::{SeqAccess, Visitor};
use std::fmt;

#[derive(Deserialize, PartialEq, Debug)]
pub struct ListDropletsResponse {
    pub droplets: Vec<DropletResponse>,
    #[serde(default)]
    pub links: Links,
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
pub struct ListAppsResponse {
    #[serde(default)]
    pub links: Links,
    #[serde(default)]
    pub apps: Vec<AppResponse>,
}

#[derive(Deserialize, PartialEq, Debug, Default)]
pub struct Links {
    #[serde(default)]
    pub pages: Pages,
}

#[derive(Deserialize, PartialEq, Debug, Default)]
pub struct Pages {
    pub prev: Option<String>,
    pub next: Option<String>,
    pub first: Option<String>,
    pub last: Option<String>,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct AppResponse {
    pub id: String,
    pub spec: AppSpec,
    pub active_deployment: Option<AppActiveDeployment>,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct AppSpec {
    pub name: String,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct AppActiveDeployment {
    pub id: String,
    pub cause: String,
    pub phase: String,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct DropletDataResponse {
    pub status: String,
    pub data: DropletDataResult,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct DropletDataResult {
    pub result: Vec<DropletMetricsResponse>,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct DropletMetricsResponse {
    pub metric: DropletMetricMetaInfo,
    #[serde(deserialize_with = "deserialize_points")]
    pub values: Vec<MetricPoint>,
}

// Of course, I could use generics here, but I think it would be much harder to work with and understand.
// So, I prefer to create a bit of chaos =)
// Also, if Digital Ocean ever decides to change the protocol, everything would continue to work, just with unknown labels.
#[derive(Deserialize, PartialEq, Debug, Default)]
pub struct DropletMetricMetaInfo {
    pub host_id: String,

    // for cpu
    pub mode: Option<String>,

    // for filesystem_free / filesystem_size
    pub device: Option<String>,
    pub fstype: Option<String>,
    pub mountpoint: Option<String>,
}

#[derive(Deserialize, PartialEq, Debug)]
#[allow(dead_code)]
pub struct DropletMetricMetaDefault {
    pub host_id: String,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct AppDataResponse {
    pub status: String,
    pub data: AppDataResult,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct AppDataResult {
    pub result: Vec<AppMetricsResponse>,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct AppMetricsResponse {
    pub metric: AppMetricMetaInfo,
    #[serde(deserialize_with = "deserialize_points")]
    pub values: Vec<MetricPoint>,
}

#[derive(Deserialize, PartialEq, Debug, Default)]
pub struct AppMetricMetaInfo {
    pub app_component: String,
    pub app_component_instance: String,
    pub app_owner_id: Option<String>,
    pub app_uuid: String,
}

#[derive(Deserialize, PartialEq, Debug)]
#[allow(dead_code)]
pub struct AppMetricMetaDefault {
    pub app_component: String,
    pub app_component_instance: String,
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
#[allow(clippy::needless_update)]
mod deserialize_test {
    use crate::client::do_json_protocol::{
        AppActiveDeployment, AppDataResponse, AppDataResult, AppMetricMetaInfo, AppMetricsResponse,
        AppResponse, AppSpec, DropletDataResponse, DropletDataResult, DropletMetricMetaInfo,
        DropletMetricsResponse, DropletResponse, Links, ListAppsResponse, ListDropletsResponse,
        MetricPoint, Pages,
    };

    #[test]
    fn deserialize_droplets() {
        let json_data = r#"{"droplets":[{"id":336239184,"name":"traefik-zitadel-1","memory":2048,"vcpus":1,"disk":50,"locked":false,"status":"active","kernel":null,"created_at":"2023-01-17T17:12:08Z","features":["monitoring","droplet_agent","private_networking"],"backup_ids":[],"next_backup_window":null,"snapshot_ids":[],"image":{"id":119383150,"name":"22.10 x64","distribution":"Ubuntu","slug":"ubuntu-22-10-x64","public":true,"regions":["nyc3","nyc1","sfo1","nyc2","ams2","sgp1","lon1","ams3","fra1","tor1","sfo2","blr1","sfo3","syd1"],"created_at":"2022-10-22T19:58:00Z","min_disk_size":7,"type":"base","size_gigabytes":0.74,"description":"Ubuntu 22.10 (LTS) x64","tags":[],"status":"available"},"volume_ids":[],"size":{"slug":"s-1vcpu-2gb","memory":2048,"vcpus":1,"disk":50,"transfer":2.0,"price_monthly":12.0,"price_hourly":0.01786,"regions":["ams3","blr1","fra1","lon1","nyc1","nyc3","sfo3","sgp1","syd1","tor1"],"available":true,"description":"Basic"},"size_slug":"s-1vcpu-2gb","networks":{"v4":[{"ip_address":"164.90.185.107","netmask":"255.255.240.0","gateway":"164.90.176.1","type":"public"},{"ip_address":"10.114.0.3","netmask":"255.255.240.0","gateway":"10.114.0.1","type":"private"}],"v6":[]},"region":{"name":"Frankfurt 1","slug":"fra1","features":["backups","ipv6","metadata","install_agent","storage","image_transfer"],"available":true,"sizes":["s-1vcpu-512mb-10gb","s-1vcpu-1gb","s-1vcpu-1gb-amd","s-1vcpu-1gb-intel","s-1vcpu-2gb","s-1vcpu-2gb-amd","s-1vcpu-2gb-intel","s-2vcpu-2gb","s-2vcpu-2gb-amd","s-2vcpu-2gb-intel","s-2vcpu-4gb","s-2vcpu-4gb-amd","s-2vcpu-4gb-intel","c-2","c2-2vcpu-4gb","s-4vcpu-8gb","s-4vcpu-8gb-amd","s-4vcpu-8gb-intel","g-2vcpu-8gb","gd-2vcpu-8gb","m-2vcpu-16gb","c-4","c2-4vcpu-8gb","s-8vcpu-16gb","m3-2vcpu-16gb","c-4-intel","s-8vcpu-16gb-amd","s-8vcpu-16gb-intel","c2-4vcpu-8gb-intel","g-4vcpu-16gb","so-2vcpu-16gb","m6-2vcpu-16gb","gd-4vcpu-16gb","so1_5-2vcpu-16gb","m-4vcpu-32gb","c-8","c2-8vcpu-16gb","m3-4vcpu-32gb","c-8-intel","c2-8vcpu-16gb-intel","g-8vcpu-32gb","so-4vcpu-32gb","m6-4vcpu-32gb","gd-8vcpu-32gb","so1_5-4vcpu-32gb","m-8vcpu-64gb","c-16","c2-16vcpu-32gb","m3-8vcpu-64gb","c-16-intel","c2-16vcpu-32gb-intel","g-16vcpu-64gb","so-8vcpu-64gb","m6-8vcpu-64gb","gd-16vcpu-64gb","so1_5-8vcpu-64gb","m-16vcpu-128gb","c-32","c2-32vcpu-64gb","m3-16vcpu-128gb","c-32-intel","c2-32vcpu-64gb-intel","m-24vcpu-192gb","g-32vcpu-128gb","so-16vcpu-128gb","m6-16vcpu-128gb","gd-32vcpu-128gb","m3-24vcpu-192gb","g-40vcpu-160gb","so1_5-16vcpu-128gb","c-48-intel","m-32vcpu-256gb","gd-40vcpu-160gb","c2-48vcpu-96gb-intel","so-24vcpu-192gb","m6-24vcpu-192gb","m3-32vcpu-256gb","so1_5-24vcpu-192gb","so-32vcpu-256gb","m6-32vcpu-256gb","so1_5-32vcpu-256gb"]},"tags":[],"vpc_uuid":"addcb62f-5973-465d-964c-4ffcac4f8b52"}],"links":{"pages":{"first":"https://api.digitalocean.com/v2/droplets?page=1\u0026per_page=1","prev":"https://api.digitalocean.com/v2/droplets?page=1\u0026per_page=1","next":"https://api.digitalocean.com/v2/droplets?page=3\u0026per_page=1","last":"https://api.digitalocean.com/v2/droplets?page=3\u0026per_page=1"}},"meta":{"total":3}}"#;
        let deserialized_data: ListDropletsResponse = serde_json::from_str(json_data).unwrap();
        let expected_result = ListDropletsResponse {
            links: Links {
                pages: Pages {
                    first: Some(
                        "https://api.digitalocean.com/v2/droplets?page=1&per_page=1".to_string(),
                    ),
                    prev: Some(
                        "https://api.digitalocean.com/v2/droplets?page=1&per_page=1".to_string(),
                    ),
                    next: Some(
                        "https://api.digitalocean.com/v2/droplets?page=3&per_page=1".to_string(),
                    ),
                    last: Some(
                        "https://api.digitalocean.com/v2/droplets?page=3&per_page=1".to_string(),
                    ),
                },
            },
            droplets: vec![DropletResponse {
                id: 336239184,
                name: "traefik-zitadel-1".to_string(),
                memory: 2048,
                vcpus: 1,
                disk: 50,
                locked: false,
                status: "active".to_string(),
            }],
        };

        assert_eq!(deserialized_data, expected_result)
    }

    #[test]
    fn deserialize_apps() {
        let json_data = r#"{"meta":{"total":11},"links":{"pages":{"last":"https://api.digitalocean.com/v2/apps?page=11&per_page=1","next":"https://api.digitalocean.com/v2/apps?page=2&per_page=1"}},"apps":[{"id":"3a8aa5b2-3d92-4d0d-9d38-3214f08f3a57","owner_uuid":"13a391ec-d5fb-477e-a5b6-c62369d38c15","spec":{"name":"AppName","services":[{"name":"web","image":{"registry_type":"DOCR","repository":"RepoName","tag":"web","deploy_on_push":{}},"instance_size_slug":"apps-s-1vcpu-1gb-fixed","instance_count":1,"http_port":8080}],"domains":[{"domain":"api.example.com","type":"PRIMARY"}],"region":"fra","alerts":[{"rule":"DEPLOYMENT_FAILED"},{"rule":"DOMAIN_FAILED"}],"ingress":{"rules":[{"match":{"path":{"prefix":"/"}},"component":{"name":"web"}}]},"features":["buildpack-stack=ubuntu-22"]},"last_deployment_active_at":"2024-08-30T13:58:33Z","default_ingress":"https://AppName.ondigitalocean.app","created_at":"2024-07-08T14:51:42Z","updated_at":"2024-09-19T13:36:02Z","active_deployment":{"id":"c079d423-e050-4a22-97cd-e9fbbbf020ad","spec":{"name":"AppName","services":[{"name":"web","image":{"registry_type":"DOCR","repository":"RepoName","tag":"web","deploy_on_push":{}},"instance_size_slug":"apps-s-1vcpu-1gb-fixed","instance_count":1,"http_port":8080}],"domains":[{"domain":"api.example.com","type":"PRIMARY"}],"region":"fra","alerts":[{"rule":"DEPLOYMENT_FAILED"},{"rule":"DOMAIN_FAILED"}],"ingress":{"rules":[{"match":{"path":{"prefix":"/"}},"component":{"name":"web"}}]},"features":["buildpack-stack=ubuntu-22"]},"services":[{"name":"web","source_image_digest":"sha256:c24c1c13e86d92ecb496c8050174af95fd5df76dbb866e34729055bd3a13bbb8"}],"phase_last_updated_at":"2024-08-30T13:58:33Z","created_at":"2024-08-30T13:57:54Z","updated_at":"2024-09-13T13:58:37Z","cause":"manual","progress":{"success_steps":6,"total_steps":6,"steps":[{"name":"build","status":"SUCCESS","steps":[{"name":"initialize","status":"SUCCESS","started_at":"2024-08-30T13:57:57.672434261Z","ended_at":"2024-08-30T13:57:57.759162219Z"},{"name":"components","status":"SUCCESS","steps":[{"name":"web","status":"SUCCESS","reason":{"code":"PreBuiltImage","message":"Your build job was skipped because you specified a pre-built image."},"component_name":"web","message_base":"Building service"}],"started_at":"2024-08-30T13:57:57.759197958Z","ended_at":"2024-08-30T13:57:57.759737867Z"}],"started_at":"2024-08-30T13:57:57.672359656Z","ended_at":"2024-08-30T13:57:57.759966408Z"},{"name":"deploy","status":"SUCCESS","steps":[{"name":"initialize","status":"SUCCESS","started_at":"2024-08-30T13:57:59.735041686Z","ended_at":"2024-08-30T13:58:00.763926223Z"},{"name":"components","status":"SUCCESS","steps":[{"name":"web","status":"SUCCESS","steps":[{"name":"deploy","status":"SUCCESS","component_name":"web","message_base":"Deploying service"},{"name":"wait","status":"SUCCESS","component_name":"web","message_base":"Waiting for service"}],"component_name":"web"}],"started_at":"2024-08-30T13:58:00.763948038Z","ended_at":"2024-08-30T13:58:31.844593165Z"},{"name":"finalize","status":"SUCCESS","started_at":"2024-08-30T13:58:32.364561249Z","ended_at":"2024-08-30T13:58:33.190573345Z"}],"started_at":"2024-08-30T13:57:59.735022449Z","ended_at":"2024-08-30T13:58:33.190642962Z"}]},"phase":"ACTIVE","tier_slug":"basic","previous_deployment_id":"923b6d07-70a1-45b5-9e46-f1ed0687764c","cause_details":{"digitalocean_user_action":{"user":{"uuid":"22b08b29-899a-458d-a788-d9c128165574","email":"admin@example.com","full_name":"AdminUser"},"name":"CREATE_DEPLOYMENT"},"type":"MANUAL"},"timing":{"pending":"3.672434261s","build_total":"0.087532147s","build_billable":"0s"}},"last_deployment_created_at":"2024-08-30T13:57:54Z","live_url":"https://api.example.com","region":{"slug":"fra","label":"Frankfurt","flag":"germany","continent":"Europe","data_centers":["fra1"]},"tier_slug":"basic","live_url_base":"https://api.example.com","live_domain":"api.example.com","domains":[{"id":"dd6c8d8e-2943-4526-95ce-686e0cd28b1a","spec":{"domain":"api.example.com","type":"PRIMARY"},"phase":"ACTIVE","progress":{"steps":[{"name":"default-ingress-ready","status":"SUCCESS","started_at":"2024-09-19T13:35:59.634545234Z"},{"name":"ensure-zone","status":"SUCCESS","started_at":"2024-09-19T13:35:59.634633033Z","ended_at":"2024-07-08T15:29:34.369563407Z"},{"name":"ensure-ns-records","status":"SUCCESS","started_at":"2024-09-19T13:35:59.634703204Z","ended_at":"2024-07-08T15:29:34.369629876Z"},{"name":"verify-nameservers","status":"SUCCESS","started_at":"2024-09-19T13:35:59.634817566Z","ended_at":"2024-07-08T15:29:34.369693003Z"},{"name":"ensure-record","status":"SUCCESS","started_at":"2024-09-19T13:35:59.634912842Z","ended_at":"2024-07-08T15:29:34.369758476Z"},{"name":"ensure-alias-record","status":"SUCCESS","started_at":"2024-09-19T13:35:59.634982353Z","ended_at":"2024-07-08T15:29:34.369836232Z"},{"name":"ensure-wildcard-record","status":"SUCCESS","started_at":"2024-09-19T13:35:59.635050384Z","ended_at":"2024-07-08T15:29:34.369917605Z"},{"name":"verify-cname","status":"SUCCESS","started_at":"2024-09-19T13:35:59.717673396Z"},{"name":"ensure-ssl-txt-record-saved","status":"SUCCESS","started_at":"2024-09-19T13:36:00.049765662Z","ended_at":"2024-07-08T15:29:34.699300167Z"},{"name":"ensure-ssl-txt-record","status":"SUCCESS","started_at":"2024-09-19T13:36:00.049962156Z","ended_at":"2024-07-08T15:29:34.699370946Z"},{"name":"ensure-renewal-email","status":"SUCCESS","started_at":"2024-09-19T13:36:00.050064902Z","ended_at":"2024-07-08T15:29:34.699419605Z"},{"name":"ensure-CA-authorization","status":"SUCCESS","started_at":"2024-09-19T13:36:00.050148486Z"},{"name":"ensure-certificate","status":"SUCCESS","started_at":"2024-09-19T13:36:00.196127105Z"},{"name":"create-deployment","status":"SUCCESS","ended_at":"2024-07-08T15:31:26.846132585Z"},{"name":"configuration-alert","status":"SUCCESS","started_at":"2024-09-19T13:36:00.595405647Z","ended_at":"2024-07-08T15:29:35.389412129Z"}]},"validation":{},"certificate_expires_at":"2024-12-04T13:48:56Z"}],"build_config":{}}]}
"#;
        let deserialized_data: ListAppsResponse = serde_json::from_str(json_data).unwrap();
        let expected_result = ListAppsResponse {
            links: Links {
                pages: Pages {
                    last: Some(
                        "https://api.digitalocean.com/v2/apps?page=11&per_page=1".to_string(),
                    ),
                    next: Some(
                        "https://api.digitalocean.com/v2/apps?page=2&per_page=1".to_string(),
                    ),
                    ..Default::default()
                },
            },
            apps: vec![AppResponse {
                id: "3a8aa5b2-3d92-4d0d-9d38-3214f08f3a57".to_string(),
                spec: AppSpec {
                    name: "AppName".to_string(),
                },
                active_deployment: Some(AppActiveDeployment {
                    id: "c079d423-e050-4a22-97cd-e9fbbbf020ad".to_string(),
                    cause: "manual".to_string(),
                    phase: "ACTIVE".to_string(),
                }),
            }],
        };

        assert_eq!(deserialized_data, expected_result)
    }

    #[test]
    fn deserialize_apps_empty_response() {
        let json_data = r#"{"meta":{"total":11},"links":{"pages":{"first":"https://api.digitalocean.com/v2/apps?page=1\u0026per_page=100","prev":"https://api.digitalocean.com/v2/apps?page=1\u0026per_page=100"}}}"#;
        let deserialized_data: ListAppsResponse = serde_json::from_str(json_data).unwrap();
        let expected_result = ListAppsResponse {
            links: Links {
                pages: Pages {
                    first: Some(
                        "https://api.digitalocean.com/v2/apps?page=1&per_page=100".to_string(),
                    ),
                    prev: Some(
                        "https://api.digitalocean.com/v2/apps?page=1&per_page=100".to_string(),
                    ),
                    ..Default::default()
                },
            },
            apps: vec![],
        };

        assert_eq!(deserialized_data, expected_result)
    }

    #[test]
    fn deserialize_droplet_metrics() {
        let json_data = r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"direction":"inbound","host_id":"335943309","interface":"public"},"values":[[1682246520,"0.00011012000000000001"],[1682246760,"0.00025643564356435644"]]}]}}"#;
        let deserialized_data: DropletDataResponse = serde_json::from_str(json_data).unwrap();
        let expected_result = DropletDataResponse {
            status: "success".into(),
            data: DropletDataResult {
                result: vec![DropletMetricsResponse {
                    metric: DropletMetricMetaInfo {
                        host_id: "335943309".into(),
                        ..Default::default()
                    },
                    values: vec![
                        MetricPoint {
                            timestamp: 1682246520,
                            value: "0.00011012000000000001".into(),
                        },
                        MetricPoint {
                            timestamp: 1682246760,
                            value: "0.00025643564356435644".into(),
                        },
                    ],
                }],
            },
        };

        assert_eq!(deserialized_data, expected_result)
    }

    #[test]
    fn deserialize_app_metrics() {
        let json_data = r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"app_component":"app-component","app_component_instance":"app-component-0","app_owner_id":"12345678","app_uuid":"671f8c90-25d4-42f5-8486-8ee23666baa6"},"values":[[1726819500,"16.846847534179688"]]},{"metric":{"app_component":"app-component","app_component_instance":"app-component-1","app_owner_id":"12345678","app_uuid":"671f8c90-25d4-42f5-8486-8ee23666baa6"},"values":[[1726819500,"16.955947875976562"]]}]}}"#;
        let deserialized_data: AppDataResponse = serde_json::from_str(json_data).unwrap();
        let expected_result = AppDataResponse {
            status: "success".into(),
            data: AppDataResult {
                result: vec![
                    AppMetricsResponse {
                        metric: AppMetricMetaInfo {
                            app_component: "app-component".into(),
                            app_component_instance: "app-component-0".into(),
                            app_owner_id: Some("12345678".into()),
                            app_uuid: "671f8c90-25d4-42f5-8486-8ee23666baa6".into(),
                            ..Default::default()
                        },
                        values: vec![MetricPoint {
                            timestamp: 1726819500,
                            value: "16.846847534179688".into(),
                        }],
                    },
                    AppMetricsResponse {
                        metric: AppMetricMetaInfo {
                            app_component: "app-component".into(),
                            app_component_instance: "app-component-1".into(),
                            app_owner_id: Some("12345678".into()),
                            app_uuid: "671f8c90-25d4-42f5-8486-8ee23666baa6".into(),
                            ..Default::default()
                        },
                        values: vec![MetricPoint {
                            timestamp: 1726819500,
                            value: "16.955947875976562".into(),
                        }],
                    },
                ],
            },
        };

        assert_eq!(deserialized_data, expected_result)
    }
}
