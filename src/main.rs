mod api;
mod config;

use {
    anyhow::Result,
    api::Api,
    config::Config,
    ethers::types::U256,
    poem::{listener::TcpListener, middleware::Cors, EndpointExt, Route, Server},
    poem_openapi::OpenApiService,
    std::{collections::HashMap, fs::create_dir_all, path::PathBuf},
    web3::types::H160,
    web3::{transports::Http, Web3},
};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let config_path = std::env::var("CONFIG_FILE_PATH")?;
    let config = Config::new(&config_path)?;
    let mut support_chain: HashMap<U256, (Web3<Http>, Vec<H160>)> = HashMap::new();
    for (web3_url, contracts) in config.support_chain {
        let web3 = Web3::new(Http::new(&web3_url)?);
        support_chain.insert(web3.eth().chain_id().await?, (web3, contracts));
    }
    let dir_path = PathBuf::from(config.dir_path);
    if !dir_path.exists() {
        create_dir_all(&dir_path)?;
    }
    let api = Api {
        support_chain,
        findora_query_url: config.findora_query_url,
        dir_path,
    };
    let api_service = OpenApiService::new(api, "zk-nft", "1.0").server(config.swagger_url);
    let ui = api_service.swagger_ui();

    println!(">>>server start<<<");
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
