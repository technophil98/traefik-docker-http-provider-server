use anyhow::anyhow;
use bollard::models::ContainerSummary;
use itertools::Itertools;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;

pub mod docker;
pub mod dynamic_configuration;

lazy_static! {
    static ref ROUTERS_LABEL_REGEX: Regex =
        Regex::new(r"traefik\.http\.routers\.(.+)\.rule").unwrap();
    static ref SERVICE_LABEL_REGEX: Regex =
        Regex::new(r"traefik\.http\.services.(.+).loadbalancer.server.port").unwrap();
}

#[derive(Debug, Clone)]
pub struct TraefikedContainer {
    pub name: String,
    pub public_ports: Vec<u16>,
    pub config: TraefikedContainerConfig,
}

#[derive(Clone, Debug)]
pub enum TraefikedContainerConfig {
    SinglePort(TraefikedContainerSinglePortConfig),
    MultiplePorts(Vec<TraefikedContainerMultiPortConfig>),
}

#[derive(Clone, Debug)]
pub struct TraefikedContainerSinglePortConfig {
    pub router_name: String,
    pub rule: String,
}

#[derive(Clone, Debug)]
pub struct TraefikedContainerMultiPortConfig {
    pub config: TraefikedContainerSinglePortConfig,
    pub service_name: String,
    pub target_port: u16,
}

impl TryFrom<ContainerSummary> for TraefikedContainer {
    type Error = anyhow::Error;

    fn try_from(value: ContainerSummary) -> Result<Self, Self::Error> {
        let name = value
            .names
            .ok_or(anyhow!("No container name found"))?
            .first()
            .expect("Container should have a name")
            .clone()[1..] // Remove leading / in container name
            .to_owned();

        let public_ports = value
            .ports
            .ok_or(anyhow!("No ports specified"))?
            .iter()
            .filter_map(|p| p.public_port)
            .collect();

        let config = value
            .labels
            .as_ref()
            .and_then(extract_traefik_config)
            .ok_or(anyhow!("Could not find a traefik rule label"))?;

        Ok(TraefikedContainer {
            name,
            public_ports,
            config,
        })
    }
}

pub(crate) fn extract_traefik_config(
    labels: &HashMap<String, String>,
) -> Option<TraefikedContainerConfig> {
    let routers: Vec<(String, String)> = labels
        .iter()
        .filter_map(|(label_key, label_value)| {
            ROUTERS_LABEL_REGEX
                .captures(label_key)
                .and_then(|captures| captures.get(1))
                .map(|router_name| (router_name.as_str().to_owned(), label_value.clone()))
        })
        .sorted_by(|a, b| Ord::cmp(&a.0, &b.0))
        .collect();

    if routers.is_empty() {
        return None;
    }

    if routers.len() == 1 {
        let (router_name, rule) = routers.first().cloned().expect("Should have an router");

        return Some(TraefikedContainerConfig::SinglePort(
            TraefikedContainerSinglePortConfig { router_name, rule },
        ));
    }

    let services: Vec<(String, u16)> = labels
        .iter()
        .filter_map(|(label_key, label_value)| {
            SERVICE_LABEL_REGEX
                .captures(label_key)
                .and_then(|captures| captures.get(1))
                .map(|service_name| service_name.as_str().to_owned())
                .and_then(|service_name| {
                    label_value
                        .parse::<u16>()
                        .ok()
                        .map(|port| (service_name, port))
                })
        })
        .sorted_by(|a, b| Ord::cmp(&a.0, &b.0))
        .collect();

    if services.len() != routers.len() {
        return None;
    }

    let multiport_configs = routers
        .into_iter()
        .zip(services)
        .map(|((router_name, rule), (service_name, target_port))| {
            TraefikedContainerMultiPortConfig {
                config: TraefikedContainerSinglePortConfig { router_name, rule },
                service_name,
                target_port,
            }
        })
        .collect();

    Some(TraefikedContainerConfig::MultiplePorts(multiport_configs))
}
