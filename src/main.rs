//! Exporteur Prometheus pour les Bbox de Bouygues Telecom (testé sur la Bbox
//! Ultym / Sagemcom F@st5696b).
//!
//! L'exporteur interroge périodiquement l'API de la Bbox en tâche de fond et
//! expose les métriques au format Prometheus sur `/metrics`.
//!
//! Configuration via variables d'environnement (un fichier `.env` est chargé
//! automatiquement s'il existe) :
//!   * `BB_PASSWORD`        (requis) — mot de passe admin, URL-encodé (voir README)
//!   * `BB_URL`             (défaut `https://mabbox.bytel.fr`)
//!   * `BB_REFRESH_SECONDS` (défaut `60`)
//!   * `BB_LISTEN`          (défaut `0.0.0.0:9100`)

mod bbox;
mod metrics;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::{Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use tokio::net::TcpListener;

use crate::bbox::BboxClient;
use crate::metrics::{HostMode, Metrics};

struct Config {
    url: String,
    password: String,
    refresh: Duration,
    listen: String,
    host_mode: HostMode,
}

impl Config {
    fn from_env() -> Result<Self> {
        let password = std::env::var("BB_PASSWORD")
            .context("la variable d'environnement BB_PASSWORD est requise")?;
        let url = std::env::var("BB_URL").unwrap_or_else(|_| "https://mabbox.bytel.fr".to_string());
        let refresh = std::env::var("BB_REFRESH_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(60);
        let listen = std::env::var("BB_LISTEN").unwrap_or_else(|_| "0.0.0.0:9100".to_string());
        let host_mode = HostMode::from_env(std::env::var("BB_HOST_METRICS").ok().as_deref());
        Ok(Self {
            url,
            password,
            refresh: Duration::from_secs(refresh.max(5)),
            listen,
            host_mode,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Charge .env si présent (sans erreur s'il est absent).
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let metrics = Arc::new(Metrics::new(config.host_mode)?);
    let client = BboxClient::new(config.url.clone(), config.password.clone())?;

    tracing::info!(url = %config.url, listen = %config.listen, refresh = ?config.refresh,
        host_metrics = ?config.host_mode, "démarrage de bb_exporter");

    // Tâche de fond : collecte périodique auprès de la Bbox.
    {
        let metrics = Arc::clone(&metrics);
        let refresh = config.refresh;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(refresh);
            loop {
                ticker.tick().await;
                let started = tokio::time::Instant::now();
                match client.collect().await {
                    Ok(snap) => {
                        let elapsed = started.elapsed().as_secs_f64();
                        metrics.update(&snap, elapsed);
                        metrics.set_scrape_success(true);
                        tracing::info!(secs = elapsed, "collecte réussie");
                    }
                    Err(e) => {
                        metrics.set_scrape_success(false);
                        tracing::error!("collecte échouée : {:#}", e);
                    }
                }
            }
        });
    }

    let app = Router::new()
        .route("/", get(index))
        .route("/metrics", get(serve_metrics))
        .with_state(Arc::clone(&metrics));

    let listener = TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("bind sur {}", config.listen))?;
    tracing::info!("métriques disponibles sur http://{}/metrics", config.listen);
    axum::serve(listener, app).await.context("serveur HTTP")?;
    Ok(())
}

async fn index() -> impl IntoResponse {
    axum::response::Html(
        "<html><head><title>bb_exporter</title></head><body>\
         <h1>bb_exporter</h1><p><a href=\"/metrics\">Metrics</a></p></body></html>",
    )
}

async fn serve_metrics(State(metrics): State<Arc<Metrics>>) -> impl IntoResponse {
    match metrics.render() {
        Ok(body) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            body,
        )
            .into_response(),
        Err(e) => {
            tracing::error!("rendu des métriques : {:#}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "erreur interne").into_response()
        }
    }
}
