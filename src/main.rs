mod api;
mod config;

use {
    anyhow::Result,
    api::Api,
    config::Config,
    poem::{listener::TcpListener, middleware::Cors, EndpointExt, Route, Server},
    poem_openapi::OpenApiService,
    std::str::FromStr,
    web3::types::H160,
    web3::{transports::Http, Web3},
};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let config_path = std::env::var("CONFIG_FILE_PATH")?;
    let config = Config::new(&config_path)?;

    let contract_address = H160::from_str(&config.contract_address.to_lowercase())?;

    let web3 = Web3::new(Http::new(&config.web3_http_url)?);

    let api = Api {
        web3,
        contract_address,
        findora_query_url: config.findora_query_url,
    };
    let api_service = OpenApiService::new(api, "zk-nft", "1.0").server(config.swagger_url);
    let ui = api_service.swagger_ui();

    log::trace!(">>>server start<<<");
    let server_addr = format!("{}:{}", config.listen_address, config.listen_port);
    Server::new(TcpListener::bind(server_addr))
        .run(
            Route::new()
                .nest("/", api_service)
                .nest("/ui", ui)
                .with(Cors::new()),
        )
        .await?;

    Ok(())
}
