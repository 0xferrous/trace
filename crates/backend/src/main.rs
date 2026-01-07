use alloy_chains::NamedChain;
use alloy_provider::{Provider, RootProvider};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::Response,
    routing::get,
};
use clap::Parser;
use dashmap::DashMap;
use foundry_evm::traces::CallTraceArena;
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tower_http::trace::TraceLayer;
use tracing::{Level, Span, level_filters::LevelFilter};
use tracing_subscriber::EnvFilter;
use url::Url;

use crate::trace::get_transaction_traces_cast;

mod trace;

#[derive(Parser, Debug)]
#[command(name = "backend")]
#[command(about = "Backend server", long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, env = "PORT", default_value = "3000")]
    port: u16,

    /// RPC URLs for chains supported
    #[arg(short, long, env = "RPC_URL", value_delimiter = ',')]
    rpc_url: Vec<Url>,
}

#[derive(Clone, Debug)]
struct Config {
    providers: DashMap<NamedChain, (Url, RootProvider)>,
    supported_chains: Vec<NamedChain>,
}

impl Config {
    async fn new(rpc_urls: &[Url]) -> eyre::Result<Self> {
        let providers = rpc_urls
            .iter()
            .map(|url| RootProvider::new_http(url.clone()))
            .collect::<Vec<_>>();
        let chain_ids = futures::future::try_join_all(
            providers
                .iter()
                .map(|p| p.get_chain_id())
                .collect::<Vec<_>>(),
        )
        .await?
        .into_iter()
        .map(|chain_id| chain_id.try_into())
        .collect::<Result<Vec<NamedChain>, _>>()?;

        let providers = chain_ids
            .clone()
            .into_iter()
            .zip(
                providers
                    .into_iter()
                    .enumerate()
                    .map(|(idx, p)| (rpc_urls[idx].clone(), p)),
            )
            .collect::<DashMap<_, _>>();

        Ok(Self {
            providers,
            supported_chains: chain_ids,
        })
    }
}

async fn get_transaction(
    State(config): State<Arc<Config>>,
    Path((chain, tx_hash)): Path<(String, String)>,
) -> Json<CallTraceArena> {
    let chain = chain.parse::<NamedChain>().unwrap();
    let provider = config.providers.get(&chain).unwrap();
    let (url, _) = provider.value();
    let arena = get_transaction_traces_cast(url, tx_hash.parse().unwrap(), false)
        .await
        .inspect_err(|e| tracing::error!("Failed to get transaction traces: {e:?}"))
        .unwrap();
    Json(arena)
}

#[derive(Serialize)]
struct ApiInfo {
    supported_chains: Vec<NamedChain>,
}

async fn get_api_info(State(config): State<Arc<Config>>) -> Json<ApiInfo> {
    Json(ApiInfo {
        supported_chains: config.supported_chains.clone(),
    })
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_writer(std::io::stdout)
        .init();

    tracing::info!("Starting backend server");

    let args = Args::parse();
    let config = Arc::new(Config::new(&args.rpc_url).await?);

    let app = Router::new()
        .route("/", get(get_api_info))
        .route("/{chain}/{tx_hash}", get(get_transaction))
        .with_state(config)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    tracing::span!(
                        Level::INFO,
                        "request",
                        method = %request.method(),
                        uri = %request.uri(),
                        version = ?request.version(),
                    )
                })
                .on_response(
                    |response: &Response<Body>, latency: Duration, span: &Span| {
                        span.record("status", tracing::field::display(response.status()));
                        tracing::info!(
                            parent: span,
                            status = %response.status(),
                            latency_ms = %latency.as_millis(),
                        );
                    },
                ),
        );

    let addr = format!("127.0.0.1:{}", args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Server running on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
