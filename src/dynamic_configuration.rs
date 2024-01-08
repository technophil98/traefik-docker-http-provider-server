use anyhow::anyhow;
use std::collections::BTreeMap;

use axum::http::{header, HeaderMap};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use url::Url;

use crate::{TraefikedContainer, TraefikedContainerConfig};

type HttpRouterName = String;

type HttpServiceName = String;

type RuleValue = String;

#[derive(Clone, Debug, Serialize)]
pub struct DynamicConfiguration {
    http: HttpConfiguration,
}

#[derive(Clone, Debug, Serialize)]
struct HttpConfiguration {
    routers: BTreeMap<HttpRouterName, HttpRouterConfiguration>,
    services: BTreeMap<HttpServiceName, HttpServiceConfiguration>,
}

#[derive(Clone, Debug, Serialize)]
struct HttpRouterConfiguration {
    rule: RuleValue,
    service: HttpServiceName,
}

#[derive(Clone, Debug, Serialize)]
struct HttpServiceConfiguration {
    #[serde(flatten)]
    service_type: HttpServiceType,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum HttpServiceType {
    LoadBalancer(LoadBalancerHttpServiceConfiguration),
}

#[derive(Clone, Debug, Serialize)]
struct LoadBalancerHttpServiceConfiguration {
    servers: Vec<ServiceUrl>,
}

#[derive(Clone, Debug, Serialize)]
struct ServiceUrl {
    url: Url,
}

impl ServiceUrl {
    pub fn new(url: Url) -> Self {
        Self { url }
    }
}

pub struct DynamicConfigurationBuilder {
    routers: BTreeMap<HttpRouterName, HttpRouterConfiguration>,
    services: BTreeMap<HttpServiceName, HttpServiceConfiguration>,
    base_url: Url,
}

impl DynamicConfigurationBuilder {
    pub fn new(base_url: Url) -> DynamicConfigurationBuilder {
        DynamicConfigurationBuilder {
            base_url,
            routers: BTreeMap::default(),
            services: BTreeMap::default(),
        }
    }

    pub fn add_container(
        mut self,
        container: &TraefikedContainer,
    ) -> anyhow::Result<DynamicConfigurationBuilder> {
        match &container.config {
            TraefikedContainerConfig::SinglePort(config) => {
                let service_name = &container.name;

                let mut url = self.base_url.clone();
                let public_port = container.public_ports.first().cloned().ok_or(anyhow!(
                    "No public port specified for container '{}'",
                    service_name
                ))?;

                url.set_port(Some(public_port))
                    .map_err(|_| anyhow!("Cannot append container public port to base_url."))?;

                self.services.insert(
                    service_name.clone(),
                    HttpServiceConfiguration {
                        service_type: HttpServiceType::LoadBalancer(
                            LoadBalancerHttpServiceConfiguration {
                                servers: vec![ServiceUrl::new(url)],
                            },
                        ),
                    },
                );

                self.routers.insert(
                    config.router_name.clone(),
                    HttpRouterConfiguration {
                        service: service_name.clone(),
                        rule: config.rule.clone(),
                    },
                );
            }
            TraefikedContainerConfig::MultiplePorts(config) => {
                for c in config {
                    let service_name = &c.service_name;

                    let mut url = self.base_url.clone();
                    url.set_port(Some(c.target_port))
                        .map_err(|_| anyhow!("Cannot append container public port to base_url."))?;

                    self.services.insert(
                        service_name.clone(),
                        HttpServiceConfiguration {
                            service_type: HttpServiceType::LoadBalancer(
                                LoadBalancerHttpServiceConfiguration {
                                    servers: vec![ServiceUrl::new(url)],
                                },
                            ),
                        },
                    );

                    self.routers.insert(
                        c.config.router_name.clone(),
                        HttpRouterConfiguration {
                            service: service_name.clone(),
                            rule: c.config.rule.clone(),
                        },
                    );
                }
            }
        }

        Ok(self)
    }

    pub fn build(self) -> DynamicConfiguration {
        DynamicConfiguration {
            http: HttpConfiguration {
                routers: self.routers,
                services: self.services,
            },
        }
    }
}

impl IntoResponse for DynamicConfiguration {
    fn into_response(self) -> Response {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, "text/yaml".parse().unwrap());

        let payload = serde_yaml::to_string(&self).expect("Should serialize to YAML");

        (headers, payload).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TraefikedContainerSinglePortConfig;

    #[test]
    fn test_yaml_serialize() -> anyhow::Result<()> {
        let dynamic_configuration = DynamicConfiguration {
            http: HttpConfiguration {
                routers: [(
                    "to-my-service".to_owned(),
                    HttpRouterConfiguration {
                        rule: "Host(`my-service.my-domain.com`)".to_owned(),
                        service: "my-service".to_owned(),
                    },
                )]
                .iter()
                .cloned()
                .collect(),
                services: [(
                    "my-service".to_owned(),
                    HttpServiceConfiguration {
                        service_type: HttpServiceType::LoadBalancer(
                            LoadBalancerHttpServiceConfiguration {
                                servers: vec![
                                    ServiceUrl {
                                        url: "http://192.168.1.100:7878".try_into()?,
                                    },
                                    ServiceUrl {
                                        url: "http://my-service.local:7878".try_into()?,
                                    },
                                ],
                            },
                        ),
                    },
                )]
                .iter()
                .cloned()
                .collect(),
            },
        };

        let expected = r#"http:
  routers:
    to-my-service:
      rule: Host(`my-service.my-domain.com`)
      service: my-service
  services:
    my-service:
      loadBalancer:
        servers:
        - url: http://192.168.1.100:7878/
        - url: http://my-service.local:7878/
"#;

        let configuration_yaml = serde_yaml::to_string(&dynamic_configuration)?;

        assert_eq!(configuration_yaml, expected);
        Ok(())
    }

    #[test]
    fn test_builder() -> anyhow::Result<()> {
        let base_url = Url::parse("http://192.168.1.100")?;
        let dynamic_configuration = DynamicConfigurationBuilder::new(base_url)
            .add_container(&TraefikedContainer {
                name: "my-service".to_owned(),
                config: TraefikedContainerConfig::SinglePort(TraefikedContainerSinglePortConfig {
                    router_name: "to-my-service".to_owned(),
                    rule: "Host(`my-service.my-domain.com`)".to_owned(),
                }),
                public_ports: vec![7878],
            })
            .build();

        let expected = r#"http:
  routers:
    to-my-service:
      rule: Host(`my-service.my-domain.com`)
      service: my-service
  services:
    my-service:
      loadBalancer:
        servers:
        - url: http://192.168.1.100:7878/
"#;

        let configuration_yaml = serde_yaml::to_string(&dynamic_configuration)?;

        assert_eq!(configuration_yaml, expected);
        Ok(())
    }
}
