use {
    anyhow::anyhow,
    ethers::{
        abi::{Function, Param, ParamType, StateMutability, Token},
        contract::{Eip712, EthAbiType},
        types::Signature,
        utils::keccak256,
    },
    finutils::txn_builder::TransactionBuilder,
    ledger::{
        data_model::{AssetRules, AssetTypeCode, AssetTypePrefix},
        store::fbnc::NumKey,
    },
    poem::Result,
    poem_openapi::{
        payload::{Json, PlainText},
        ApiResponse, Object, OpenApi, Tags,
    },
    serde::{Deserialize, Serialize},
    serde_json::Value,
    web3::{
        transports::Http,
        types::{Bytes, CallRequest, H160, U256},
        Web3,
    },
    zei::{
        serialization::ZeiFromToBytes,
        setup::PublicParams,
        xfr::{asset_record::AssetRecordType, sig::XfrPublicKey},
    },
};
pub struct Api {
    pub web3: Web3<Http>,
    pub contract_address: H160,
    pub findora_query_url: String,
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

#[derive(Eip712, EthAbiType, Clone)]

struct Issue {
    pub receive_public_key: Vec<u8>,
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
        let (address, _pub_key) =
            match get_address_and_pub_key(&req.0.receive_public_key, &req.0.signature) {
                Ok(v) => v,
                Err((code, msg)) => {
                    resp.code = code;
                    resp.msg = msg;
                    return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
                }
            };
        let balance = match get_erc_balance(&self.web3, self.contract_address, address).await {
            Ok(v) => v,
            Err((code, msg)) => {
                resp.code = code;
                resp.msg = msg;
                return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
            }
        };

        let mut data = vec![];
        data.extend(address.0);
        data.extend(self.contract_address.0);
        let chain_id = match self.web3.eth().chain_id().await {
            Ok(v) => v,
            Err(e) => {
                resp.code = -30;
                resp.msg = format!("error: {:?}", e);
                return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
            }
        };

        let mut tmp: [u8; 32] = [0; 32];
        chain_id.to_big_endian(&mut tmp);
        data.extend(&tmp);
        balance.to_big_endian(&mut tmp);
        data.extend(tmp);

        let builder =
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
                resp.code = -40;
                resp.msg = format!("error: {:?}", e);
                return Ok(GetIssueTxRespEnum::Ok(Json(resp)));
            }
        };
        Ok(GetIssueTxRespEnum::Ok(Json(resp)))
    }
}
fn create_asset_tx(
    url: &str,
    code: &[u8],
    amount: u64,
) -> Result<TransactionBuilder, (i32, String)> {
    let code = AssetTypeCode::from_bytes(code).map_err(|e| (-21, format!("error: {:?}", e)))?;

    let asset_code =
        AssetTypeCode::from_prefix_and_raw_asset_type_code(AssetTypePrefix::UserDefined, &code);

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
    Ok(builder)
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

fn get_address_and_pub_key(
    receive_public_key: &str,
    signature: &str,
) -> Result<(H160, XfrPublicKey), (i32, String)> {
    let s = receive_public_key
        .strip_prefix("0x")
        .unwrap_or(receive_public_key);

    let fra_pub_key = hex::decode(s)
        .map_err(|e| (-3, format!("error: {:?}", e)))
        .and_then(|v| {
            if v.len() != 32 {
                Err((
                    -1,
                    format!("The length of the public key is not 32 bytes: {}", v.len()),
                ))
            } else {
                Ok(v)
            }
        })?;

    let s = signature.strip_prefix("0x").unwrap_or(signature);
    let signature = hex::decode(s)
        .map_err(|e| (-3, format!("error: {:?}", e)))
        .and_then(|v| {
            Signature::try_from(v.as_slice()).map_err(|e| (-4, format!("error: {:?}", e)))
        })?;

    let address = signature
        .recover_typed_data(&Issue {
            receive_public_key: fra_pub_key.to_vec(),
        })
        .map_err(|e| (-5, format!("error: {:?}", e)))?;

    let pub_key =
        XfrPublicKey::zei_from_bytes(&fra_pub_key).map_err(|e| (-6, format!("error: {:?}", e)))?;
    Ok((address, pub_key))
}
