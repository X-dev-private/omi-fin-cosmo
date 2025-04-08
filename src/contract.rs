use cosmwasm_std::{
    entry_point, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
    Addr, Timestamp, Decimal, StdError, to_json_binary
};
use cw2::set_contract_version;
use cw20::{Cw20Coin, Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse, Cw20QueryMsg, BalanceResponse, TokenInfoResponse};
use cw_controllers::Admin;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const CONTRACT_NAME: &str = "crates.io:custom-token";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct InstantiateMsg {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub initial_supply: Uint128,
    pub fee_receiver: String,
    pub owner: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Transfer { recipient: String, amount: Uint128 },
    Burn { amount: Uint128 },
    Send { contract: String, amount: Uint128, msg: Binary },
    Mint {},
    IncreaseAllowance { spender: String, amount: Uint128, expires: Option<u64> },
    DecreaseAllowance { spender: String, amount: Uint128, expires: Option<u64> },
    TransferFrom { owner: String, recipient: String, amount: Uint128 },
    SetMintAmount { amount: Uint128 },
    SetMintEnabled { enabled: bool },
    LockOwnership {},
    TransferOwnership { new_owner: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Balance { address: String },
    TokenInfo {},
    Minter {},
    Allowance { owner: String, spender: String },
    GetConfig {},
    GetMintInfo { address: String },
    GetTotalBurned {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Config {
    pub mint_interval_seconds: u64,
    pub fee_percent: Decimal,
    pub fee_receiver: Addr,
    pub immutable_mode: bool,
    pub mint_enabled: bool,
    pub mint_amount: Uint128,
    pub owner: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MintInfo {
    pub last_mint_time: Timestamp,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct TotalBurnedResponse {
    pub total_burned: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Uint128,
}

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, StdError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let owner = match msg.owner {
        Some(owner) => deps.api.addr_validate(&owner)?,
        None => info.sender,
    };

    let fee_receiver = deps.api.addr_validate(&msg.fee_receiver)?;

    let config = Config {
        mint_interval_seconds: 24 * 60 * 60,
        fee_percent: Decimal::percent(1),
        fee_receiver: fee_receiver.clone(),
        immutable_mode: false,
        mint_enabled: false,
        mint_amount: Uint128::from(400_000_000_000_000_000u128),
        owner: owner.clone(),
    };

    CONFIG.save(deps.storage, &config)?;
    TOTAL_BURNED.save(deps.storage, &Uint128::zero())?;

    let token_info = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply: msg.initial_supply,
    };

    TOKEN_INFO.save(deps.storage, &token_info)?;
    BALANCES.save(deps.storage, &owner, &msg.initial_supply)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", owner)
        .add_attribute("initial_supply", msg.initial_supply))
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, StdError> {
    match msg {
        ExecuteMsg::Transfer { recipient, amount } => execute_transfer(deps, env, info, recipient, amount),
        ExecuteMsg::Burn { amount } => execute_burn(deps, env, info, amount),
        ExecuteMsg::Mint {} => execute_mint(deps, env, info),
        ExecuteMsg::SetMintAmount { amount } => execute_set_mint_amount(deps, env, info, amount),
        ExecuteMsg::SetMintEnabled { enabled } => execute_set_mint_enabled(deps, env, info, enabled),
        ExecuteMsg::LockOwnership {} => execute_lock_ownership(deps, env, info),
        ExecuteMsg::TransferOwnership { new_owner } => execute_transfer_ownership(deps, env, info, new_owner),
        ExecuteMsg::TransferFrom { owner, recipient, amount } => execute_transfer_from(deps, env, info, owner, recipient, amount),
        _ => Err(StdError::generic_err("Unsupported execute message")),
    }
}

fn execute_transfer(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let recipient_addr = deps.api.addr_validate(&recipient)?;
    
    let fee = amount * config.fee_percent;
    let amount_after_fee = amount.checked_sub(fee)?;

    BALANCES.update(deps.storage, &info.sender, |balance| -> StdResult<_> {
        balance.unwrap_or_default().checked_sub(amount)
    })?;

    BALANCES.update(deps.storage, &recipient_addr, |balance| -> StdResult<_> {
        balance.unwrap_or_default().checked_add(amount_after_fee)
    })?;

    BALANCES.update(deps.storage, &config.fee_receiver, |balance| -> StdResult<_> {
        balance.unwrap_or_default().checked_add(fee)
    })?;

    Ok(Response::new()
        .add_attribute("action", "transfer")
        .add_attribute("from", info.sender)
        .add_attribute("to", recipient)
        .add_attribute("amount", amount_after_fee)
        .add_attribute("fee", fee))
}

fn execute_transfer_from(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: String,
    recipient: String,
    amount: Uint128,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let owner_addr = deps.api.addr_validate(&owner)?;
    let recipient_addr = deps.api.addr_validate(&recipient)?;
    
    let fee = amount * config.fee_percent;
    let amount_after_fee = amount.checked_sub(fee)?;

    ALLOWANCES.update(deps.storage, (&owner_addr, &info.sender), |allowance| {
        let mut allowance = allowance.unwrap_or_default();
        allowance.allowance = allowance.allowance.checked_sub(amount)?;
        Ok(allowance)
    })?;

    BALANCES.update(deps.storage, &owner_addr, |balance| -> StdResult<_> {
        balance.unwrap_or_default().checked_sub(amount)
    })?;

    BALANCES.update(deps.storage, &recipient_addr, |balance| -> StdResult<_> {
        balance.unwrap_or_default().checked_add(amount_after_fee)
    })?;

    BALANCES.update(deps.storage, &config.fee_receiver, |balance| -> StdResult<_> {
        balance.unwrap_or_default().checked_add(fee)
    })?;

    Ok(Response::new()
        .add_attribute("action", "transfer_from")
        .add_attribute("from", owner)
        .add_attribute("to", recipient)
        .add_attribute("by", info.sender)
        .add_attribute("amount", amount_after_fee)
        .add_attribute("fee", fee))
}

fn execute_burn(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    
    if info.sender != config.owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    if amount.is_zero() {
        return Err(StdError::generic_err("Amount must be greater than 0"));
    }

    BALANCES.update(deps.storage, &info.sender, |balance| -> StdResult<_> {
        balance.unwrap_or_default().checked_sub(amount)
    })?;

    TOTAL_BURNED.update(deps.storage, |total| -> StdResult<_> {
        Ok(total + amount)
    })?;

    TOKEN_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.total_supply = info.total_supply.checked_sub(amount)?;
        Ok(info)
    })?;

    Ok(Response::new()
        .add_attribute("action", "burn")
        .add_attribute("from", info.sender)
        .add_attribute("amount", amount))
}

fn execute_mint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    
    if !config.mint_enabled {
        return Err(StdError::generic_err("Mint is disabled"));
    }

    let mut mint_info = MINT_INFO.may_load(deps.storage, &info.sender)?.unwrap_or(MintInfo {
        last_mint_time: Timestamp::from_seconds(0),
    });

    let current_time = env.block.time;
    let next_mint_time = mint_info.last_mint_time.plus_seconds(config.mint_interval_seconds);

    if current_time < next_mint_time {
        return Err(StdError::generic_err("You have already minted recently. Please wait."));
    }

    mint_info.last_mint_time = current_time;
    MINT_INFO.save(deps.storage, &info.sender, &mint_info)?;

    BALANCES.update(deps.storage, &info.sender, |balance| -> StdResult<_> {
        balance.unwrap_or_default().checked_add(config.mint_amount)
    })?;

    TOKEN_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.total_supply = info.total_supply.checked_add(config.mint_amount)?;
        Ok(info)
    })?;

    Ok(Response::new()
        .add_attribute("action", "mint")
        .add_attribute("to", info.sender)
        .add_attribute("amount", config.mint_amount))
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Balance { address } => {
            let address = deps.api.addr_validate(&address)?;
            let balance = BALANCES
                .may_load(deps.storage, &address)?
                .unwrap_or_default();
            to_json_binary(&BalanceResponse { balance })
        }
        QueryMsg::TokenInfo {} => {
            let info = TOKEN_INFO.load(deps.storage)?;
            to_json_binary(&TokenInfoResponse {
                name: info.name,
                symbol: info.symbol,
                decimals: info.decimals,
                total_supply: info.total_supply,
            })
        }
        QueryMsg::GetConfig {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::GetMintInfo { address } => {
            let addr = deps.api.addr_validate(&address)?;
            let mint_info = MINT_INFO.may_load(deps.storage, &addr)?.unwrap_or(MintInfo {
                last_mint_time: Timestamp::from_seconds(0),
            });
            to_json_binary(&mint_info)
        }
        QueryMsg::GetTotalBurned {} => {
            let total_burned = TOTAL_BURNED.load(deps.storage)?;
            to_json_binary(&TotalBurnedResponse { total_burned })
        }
        _ => Err(StdError::generic_err("Unsupported query message")),
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct AllowanceResponse {
    pub allowance: Uint128,
    pub expires: u64,
}

pub const TOKEN_INFO: Item<TokenInfo> = Item::new("token_info");
pub const BALANCES: Map<&Addr, Uint128> = Map::new("balances");
pub const ALLOWANCES: Map<(&Addr, &Addr), AllowanceResponse> = Map::new("allowances");
pub const CONFIG: Item<Config> = Item::new("config");
pub const MINT_INFO: Map<&Addr, MintInfo> = Map::new("mint_info");
pub const TOTAL_BURNED: Item<Uint128> = Item::new("total_burned");