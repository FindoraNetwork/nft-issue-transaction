use {
    anyhow::anyhow,
    ethers::{
        abi::{Function, Param, ParamType, StateMutability, Token},
        types::Signature,
        utils::keccak256,
    },
    finutils::txn_builder::TransactionBuilder,
    ledger::{
        data_model::{AssetRules, AssetTypeCode},
        store::fbnc::NumKey,
    },
    poem::{web::Path, Result},
    poem_openapi::{
        payload::{Json, PlainText},
        ApiResponse, Object, OpenApi, Tags,
    },
    serde::{Deserialize, Serialize},
    serde_json::Value,
    std::{
        collections::HashMap,
        fs::{read_to_string, File},
        io::Write,
        path::PathBuf,
        str::FromStr,
    },
    web3::{
        transports::Http,
        types::{Bytes, CallRequest, H160, U256},
        Web3,
    },
    zei::{setup::PublicParams, xfr::asset_record::AssetRecordType},
};
pub struct Api {
    pub findora_query_url: String,
    pub support_chain: HashMap<U256, (Web3<Http>, Vec<H160>)>,
    pub dir_path: PathBuf,
}

#[derive(Tags)]
enum ApiTags {
    Version,
    Transaction,
}
#[derive(Serialize, Deserialize, Debug, Object, Clone)]
pub struct VersionResp {
    pub git_commit: String,
    pub git_semver: String,
    pub rustc_commit: String,
    pub rustc_semver: String,
}

#[derive(ApiResponse)]
pub enum VersionRespEnum {
    #[oai(status = 200)]
    Ok(Json<VersionResp>),
}
#[derive(ApiResponse)]
pub enum PingRespEnum {
    #[oai(status = 200)]
    Ok(PlainText<String>),
}

#[derive(Serialize, Deserialize, Debug, Object, Clone)]
pub struct GetIssueTxReq {
    pub id: String,
    pub receive_public_key: String,
    pub signature: String,
    pub chainid: String,
    pub token_address: String,
    pub tokenid: String,
    pub is_721: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rand_str: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Object, Clone)]
pub struct GetIssueTxResp {
    pub id: String,
    pub code: i32,
    pub msg: String,
}

#[derive(ApiResponse)]
pub enum GetIssueTxRespEnum {
    #[oai(status = 200)]
    Ok(Json<GetIssueTxResp>),
}

#[derive(ApiResponse)]
pub enum GetSupportChain {
    #[oai(status = 200)]
    Ok(Json<HashMap<String, Vec<String>>>),
}

#[derive(ApiResponse)]
pub enum GetIssueInfo {
    #[oai(status = 200)]
    Ok(Json<String>),
}

#[OpenApi]
impl Api {
    #[oai(path = "/version", method = "get", tag = "ApiTags::Version")]
    async fn version(&self) -> Result<VersionRespEnum> {
        let resp = VersionResp {
            git_commit: env!("VERGEN_GIT_SHA").to_string(),
            git_semver: env!("VERGEN_GIT_SEMVER").to_string(),
            rustc_commit: env!("VERGEN_RUSTC_COMMIT_HASH").to_string(),
            rustc_semver: env!("VERGEN_RUSTC_SEMVER").to_string(),
        };

        Ok(VersionRespEnum::Ok(Json(resp)))
    }

    #[oai(path = "/ping", method = "get", tag = "ApiTags::Version")]
    async fn ping(&self) -> Result<PingRespEnum> {
        Ok(PingRespEnum::Ok(PlainText(String::from("pong"))))
    }

    #[oai(
        path = "/get_support_chain",
        method = "get",
        tag = "ApiTags::Transaction"
    )]
    async fn get_support_chain(&self) -> Result<GetSupportChain> {
        let mut chain = HashMap::new();
        for (chainid, (_, contracts)) in self.support_chain.clone() {
            chain.insert(
                format!("{:?}", chainid),
                contracts.iter().map(|v| format!("{:?}", v)).collect(),
            );
        }

        Ok(GetSupportChain::Ok(Json(chain)))
    }
    #[oai(
        path = "/get_issue_info/:hex_code",
        method = "get",
        tag = "ApiTags::Transaction"
    )]
    async fn get_issue_info(&self, hex_code: Path<String>) -> Result<GetIssueInfo> {
        let hex_code = hex_code.strip_prefix("0x").unwrap_or(&hex_code);
        let file_name = self.dir_path.join(hex_code);
        Ok(GetIssueInfo::Ok(Json(
            read_to_string(file_name).unwrap_or_default(),
        )))
    }

    #[oai(
        path = "/get_issue_transaction",
        method = "post",
        tag = "ApiTags::Transaction"
    )]
    async fn get_issue_transaction(&self, req: Json<GetIssueTxReq>) -> Result<GetIssueTxRespEnum> {
        let mut resp = GetIssueTxResp {
            id: req.0.id.clone(),
            code: 0,
            msg: String::new(),
        };
        let address = match get_address_and_pub_key(&req.0.receive_public_key, &req.0.signature) {
            Ok(v) => v,
            Err((code, msg)) => {
                resp.code = code;
                resp.msg = msg;
                return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
            }
        };
        let chainid = match U256::from_str(&req.chainid) {
            Ok(v) => v,
            Err(e) => {
                resp.code = -30;
                resp.msg = format!("chainid format error:{:?}", e);
                return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
            }
        };
        let token_address = match H160::from_str(&req.token_address) {
            Ok(v) => v,
            Err(e) => {
                resp.code = -31;
                resp.msg = format!("token_address format error:{:?}", e);
                return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
            }
        };

        let (web3, contract_address) = match self.support_chain.get(&chainid) {
            Some(v) => v,
            None => {
                resp.code = -32;
                resp.msg = String::from("chain not support");
                return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
            }
        };
        if !contract_address.contains(&token_address) {
            resp.code = -33;
            resp.msg = String::from("token_address not support");
            return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
        }
        let tokenid = match U256::from_str(&req.tokenid) {
            Ok(v) => v,
            Err(e) => {
                resp.code = -35;
                resp.msg = format!("tokenid format error:{:?}", e);
                return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
            }
        };
        let mut balance = if req.is_721 {
            match get_erc_balance(&web3, token_address, address).await {
                Ok(v) => v,
                Err((code, msg)) => {
                    resp.code = code;
                    resp.msg = msg;
                    return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
                }
            }
        } else {
            match get_1155_balance(&web3, token_address, address, tokenid).await {
                Ok(v) => v,
                Err((code, msg)) => {
                    resp.code = code;
                    resp.msg = msg;
                    return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
                }
            }
        };

        if balance > U256::from(u64::MAX) {
            balance = U256::from(u64::MAX);
        }
        if balance.is_zero() {
            resp.code = -36;
            resp.msg = format!(
                "balance is zero account: {:?} chainid:{} contract_address:{:?}",
                address, chainid, token_address
            );
            return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
        }
        let mut data = vec![];
        {
            data.extend(token_address.0);
            let chain_id = match web3.eth().chain_id().await {
                Ok(v) => v,
                Err(e) => {
                    resp.code = -40;
                    resp.msg = format!("error: {:?}", e);
                    return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
                }
            };
            let mut tmp: [u8; 32] = [0; 32];
            tokenid.to_big_endian(&mut tmp);
            data.extend(&tmp);
            tmp = [0; 32];
            chain_id.to_big_endian(&mut tmp);
            data.extend(&tmp);
            if let Some(v) = &req.rand_str {
                data.extend(v.as_bytes());
            }
        }

        let (builder, asset_code) =
            match create_asset_tx(&self.findora_query_url, &keccak256(data), balance.as_u64()) {
                Ok(v) => v,
                Err((code, msg)) => {
                    resp.code = code;
                    resp.msg = msg;
                    return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
                }
            };
        resp.msg = match serde_json::to_string(&builder) {
            Ok(v) => v,
            Err(e) => {
                resp.code = -50;
                resp.msg = format!("error: {:?}", e);
                return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
            }
        };
        let hex_code = hex::encode(&asset_code);
        let file_name = self.dir_path.join(hex_code);
        let mut file = match File::create(file_name) {
            Ok(v) => v,
            Err(e) => {
                resp.code = -60;
                resp.msg = format!("save file error: {:?}", e);
                return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
            }
        };
        let json = match serde_json::to_string(&req.0) {
            Ok(v) => v,
            Err(e) => {
                resp.code = -70;
                resp.msg = format!("save file error: {:?}", e);
                return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
            }
        };
        if let Err(e) = file.write_all(json.as_bytes()) {
            resp.code = -80;
            resp.msg = format!("save file error: {:?}", e);
            return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
        };

        Ok(GetIssueTxRespEnum::Ok(Json(resp)))
    }
}

fn create_asset_tx(
    url: &str,
    code: &[u8],
    amount: u64,
) -> Result<(TransactionBuilder, Vec<u8>), (i32, String)> {
    let code = AssetTypeCode::from_bytes(code).map_err(|e| (-21, format!("error: {:?}", e)))?;

    let asset_code = get_derived_asset_code(url, &code).map_err(|e| (-21, format!("error: {:?}", e)))?;

    let mut rules = AssetRules::default();
    let decimal = 6;
    let max_units = None;
    let transferable = true;
    rules
        .set_decimals(decimal)
        .map_err(|e| (-22, format!("error: {:?}", e)))?;
    rules.set_max_units(max_units);
    rules.set_transferable(transferable);

    let mnemonic = globutils::wallet::generate_mnemonic_custom(24, "en")
        .map_err(|e| (-23, format!("error: {:?}", e)))?;
    let kp = globutils::wallet::restore_keypair_from_mnemonic_default(&mnemonic)
        .map_err(|e| (-24, format!("error: {:?}", e)))?;

    let memo = String::new();

    let mut builder = get_transaction_builder(url).map_err(|e| (-25, format!("error: {:?}", e)))?;
    builder
        .add_operation_create_asset(&kp, Some(code), rules, &memo)
        .map_err(|e| (-26, format!("error: {:?}", e)))?;

    builder
        .add_basic_issue_asset(
            &kp,
            &asset_code,
            builder.get_seq_id(),
            amount,
            AssetRecordType::NonConfidentialAmount_NonConfidentialAssetType,
            &PublicParams::default(),
        )
        .map_err(|e| (-27, format!("error: {:?}", e)))?;
    Ok((builder, asset_code.val.0.to_vec()))
}

fn get_transaction_builder(url: &str) -> anyhow::Result<TransactionBuilder> {
    let url = format!("{}/global_state", url);
    attohttpc::get(&url)
        .send()
        .and_then(|resp| resp.error_for_status())
        .and_then(|resp| resp.bytes())
        .map_err(|e| anyhow!("{:?}", e))
        .and_then(|bytes| {
            serde_json::from_slice::<(Value, u64, Value)>(&bytes).map_err(|e| anyhow!("{:?}", e))
        })
        .map(|resp| TransactionBuilder::from_seq_id(resp.1))
}

fn get_derived_asset_code(url: &str, raw_asset_code: &AssetTypeCode) -> anyhow::Result<AssetTypeCode> {
    let url = format!("{}/get_derived_asset_code/{}", url, raw_asset_code.to_base64());
    attohttpc::get(&url)
    .send()
    .and_then(|resp| resp.error_for_status())
    .and_then(|resp| resp.text())
    .map_err(|e| anyhow!("{:?}", e))
    .and_then(|str| {
        AssetTypeCode::new_from_base64(str.as_str()).map_err(|e| anyhow!("{:?}", e))
    })
}

async fn get_erc_balance(
    web3: &Web3<Http>,
    contract_address: H160,
    address: H160,
) -> anyhow::Result<U256, (i32, String)> {
    #[allow(deprecated)]
    let function = Function {
        name: String::from("balanceOf"),
        inputs: vec![Param {
            name: String::from("account"),
            kind: ParamType::Address,
            internal_type: Some(String::from("address")),
        }],
        outputs: vec![Param {
            name: String::new(),
            kind: ParamType::Uint(256),
            internal_type: Some(String::from("uint256")),
        }],
        constant: None,
        state_mutability: StateMutability::Payable,
    };
    let data = function
        .encode_input(&vec![Token::Address(address)])
        .map_err(|e| (-11, format!("error: {:?}", e)))?;

    let bytes = web3
        .eth()
        .call(
            CallRequest {
                to: Some(contract_address),
                data: Some(Bytes(data)),
                ..Default::default()
            },
            None,
        )
        .await
        .map_err(|e| (-12, format!("error: {:?}", e)))?;

    let vts = function
        .decode_output(&bytes.0)
        .map_err(|e| (-13, format!("error: {:?}", e)))?;

    let t = vts
        .get(0)
        .cloned()
        .ok_or_else(|| (-14, String::from("balance not found")))?;

    if let Token::Uint(v) = t {
        Ok(v)
    } else {
        Err((-15, String::from("balance return type error")))
    }
}

async fn get_1155_balance(
    web3: &Web3<Http>,
    contract_address: H160,
    address: H160,
    tokenid: U256,
) -> anyhow::Result<U256, (i32, String)> {
    #[allow(deprecated)]
    let function = Function {
        name: String::from("balanceOf"),
        inputs: vec![
            Param {
                name: String::from("account"),
                kind: ParamType::Address,
                internal_type: Some(String::from("address")),
            },
            Param {
                name: String::from("id"),
                kind: ParamType::Uint(256),
                internal_type: Some(String::from("uint256")),
            },
        ],
        outputs: vec![Param {
            name: String::new(),
            kind: ParamType::Uint(256),
            internal_type: Some(String::from("uint256")),
        }],
        constant: None,
        state_mutability: StateMutability::Payable,
    };
    let data = function
        .encode_input(&vec![Token::Address(address), Token::Uint(tokenid)])
        .map_err(|e| (-11, format!("error: {:?}", e)))?;

    let bytes = web3
        .eth()
        .call(
            CallRequest {
                to: Some(contract_address),
                data: Some(Bytes(data)),
                ..Default::default()
            },
            None,
        )
        .await
        .map_err(|e| (-12, format!("error: {:?}", e)))?;

    let vts = function
        .decode_output(&bytes.0)
        .map_err(|e| (-13, format!("error: {:?}", e)))?;

    let t = vts
        .get(0)
        .cloned()
        .ok_or_else(|| (-14, String::from("balance not found")))?;

    if let Token::Uint(v) = t {
        Ok(v)
    } else {
        Err((-15, String::from("balance return type error")))
    }
}

fn get_address_and_pub_key(message: &str, signature: &str) -> Result<H160, (i32, String)> {
    let s = signature.strip_prefix("0x").unwrap_or(signature);
    let signature = hex::decode(s)
        .map_err(|e| (-3, format!("error: {:?}", e)))
        .and_then(|v| {
            Signature::try_from(v.as_slice()).map_err(|e| (-4, format!("error: {:?}", e)))
        })?;

    let address = signature
        .recover(message)
        .map_err(|e| (-5, format!("error: {:?}", e)))?;

    Ok(address)
}
