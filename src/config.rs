use {
    anyhow::Result,
    serde::{Deserialize, Serialize},
    std::{fs::File, io::Read},
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub swagger_url: String,
    pub listen_address: String,
    pub listen_port: u16,
    pub findora_query_url: String,
    pub web3_http_url: String,
    pub contract_address: String,
}
impl Config {
    pub fn new(path: &str) -> Result<Self> {
        let mut file = File::open(path)?;

        let mut str = String::new();
        file.read_to_string(&mut str)?;

        let config: Config = toml::from_str(&str)?;
        Ok(config)
    }
}
