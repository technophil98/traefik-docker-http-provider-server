use crate::{extract_traefik_config, TraefikedContainer};
use bollard::Docker;

pub async fn get_traefik_labeled_containers() -> anyhow::Result<Vec<TraefikedContainer>> {
    let docker = Docker::connect_with_local_defaults()?;

    let containers = docker
        .list_containers::<String>(None)
        .await?
        .iter()
        .filter(|c| c.labels.as_ref().and_then(extract_traefik_config).is_some())
        .cloned()
        .filter_map(|c| c.try_into().ok())
        .collect();

    Ok(containers)
}

#[cfg(test)]
mod tests {
    use assertables::*;
    use rstest::*;

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_get_traefik_labeled_containers() -> anyhow::Result<()> {
        let containers = get_traefik_labeled_containers().await?;

        println!("{:?}", containers);

        let container_names: Vec<String> = containers.into_iter().map(|c| c.name).collect();

        assert_contains!(container_names, &String::from("nginx1"));
        assert_contains!(container_names, &String::from("nginx2"));
        assert_not_contains!(container_names, &String::from("nginx3"));

        Ok(())
    }
}
