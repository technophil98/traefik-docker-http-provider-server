use anyhow::Context;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde_json::json;
use thiserror::Error;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use url::Url;

use traefik_docker_http_provider_server::docker::get_traefik_labeled_containers;
use traefik_docker_http_provider_server::dynamic_configuration::{
    DynamicConfiguration, DynamicConfigurationBuilder,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "traefik_docker_http_provider_server=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();

    tracing::info!("listening on {}", listener.local_addr().unwrap());

    let app = app()?;

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

fn app() -> anyhow::Result<Router> {
    let app = Router::new()
        .route("/", get(health_check))
        .route("/dynamic_configuration", get(dynamic_configuration))
        .layer(TraceLayer::new_for_http())
        .layer(Extension(
            std::env::var("BASE_URL")
                .context("Cannot get base URL")?
                .parse::<Url>()?,
        ));

    Ok(app)
}

async fn health_check() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

async fn dynamic_configuration(
    Extension(base_url): Extension<Url>,
) -> Result<DynamicConfiguration, AppError> {
    let labeled_containers = get_traefik_labeled_containers().await?;

    let mut dynamic_configuration_builder = DynamicConfigurationBuilder::new(base_url);
    for container in &labeled_containers {
        dynamic_configuration_builder = dynamic_configuration_builder.add_container(container)?
    }

    Ok(dynamic_configuration_builder.build())
}

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error(transparent)]
    DockerError(#[from] bollard::errors::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::DockerError(docker_error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal Docker error: {}", docker_error),
            ),
            AppError::Other(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Something went wrong: {}", e),
            ),
        };

        let json_payload = json!({"error": message });
        (status, Json(json_payload)).into_response()
    }
}
