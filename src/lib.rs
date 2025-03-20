use cosmwasm_std::{
    entry_point, to_json_binary, Addr, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
    BankMsg, Coin,
};
use cw_storage_plus::{Item, Map};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InstantiateMsg {
    pub owner: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ExecuteMsg {
    CreateToken {
        name: String,
        symbol: String,
        initial_supply: Uint128,
    },
    Mint {
        token_address: String,
    },
    Transfer {
        token_address: String,
        recipient: String,
        amount: Uint128,
    },
    SetMintEnabled {
        token_address: String,
        enabled: bool,
    },
    LockOwnership {
        token_address: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub supply: Uint128,
    pub fee_receiver: Addr,
    pub creator: Addr,
    pub mint_enabled: bool,
    pub immutable_mode: bool,
}

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] cosmwasm_std::StdError),
    #[error("Unauthorized")]
    Unauthorized {},
    #[error("Mint is disabled")]
    MintDisabled {},
    #[error("Contract is locked")]
    ContractLocked {},
}

pub const OWNER: Item<Addr> = Item::new("owner");
pub const TOKENS: Map<&Addr, Vec<Addr>> = Map::new("tokens");
pub const ALL_TOKENS: Item<Vec<Addr>> = Item::new("all_tokens");
pub const TOKEN_INFO: Map<&Addr, TokenInfo> = Map::new("token_info");

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let owner = deps.api.addr_validate(&msg.owner)?;
    OWNER.save(deps.storage, &owner)?;
    Ok(Response::new())
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::CreateToken {
            name,
            symbol,
            initial_supply,
        } => create_token(deps, info, name, symbol, initial_supply),
        ExecuteMsg::Mint { token_address } => mint(deps, info, token_address),
        ExecuteMsg::Transfer {
            token_address,
            recipient,
            amount,
        } => transfer(deps, info, token_address, recipient, amount),
        ExecuteMsg::SetMintEnabled {
            token_address,
            enabled,
        } => set_mint_enabled(deps, info, token_address, enabled),
        ExecuteMsg::LockOwnership { token_address } => lock_ownership(deps, info, token_address),
    }
}

fn create_token(
    deps: DepsMut,
    info: MessageInfo,
    name: String,
    symbol: String,
    initial_supply: Uint128,
) -> Result<Response, ContractError> {
    let owner = OWNER.load(deps.storage)?;
    let token_address = deps.api.addr_validate(&info.sender.to_string())?;

    let token_info = TokenInfo {
        name,
        symbol,
        supply: initial_supply,
        fee_receiver: owner.clone(),
        creator: info.sender.clone(),
        mint_enabled: false,
        immutable_mode: false,
    };

    TOKEN_INFO.save(deps.storage, &token_address, &token_info)?;
    ALL_TOKENS.update(deps.storage, |mut all_tokens| -> StdResult<_> {
        all_tokens.push(token_address.clone());
        Ok(all_tokens)
    })?;

    // Corrigindo erro de Option<Vec> no TOKENS
    TOKENS.update(deps.storage, &info.sender, |tokens| -> StdResult<_> {
        let mut tokens = tokens.unwrap_or_default();
        tokens.push(token_address.clone());
        Ok(tokens)
    })?;

    Ok(Response::new().add_attribute("action", "create_token"))
}

fn mint(
    deps: DepsMut,
    _info: MessageInfo,
    token_address: String,
) -> Result<Response, ContractError> {
    let token_address = deps.api.addr_validate(&token_address)?;
    let mut token_info = TOKEN_INFO.load(deps.storage, &token_address)?;

    if !token_info.mint_enabled {
        return Err(ContractError::MintDisabled {});
    }

    let mint_amount = Uint128::new(40_000_000_000_000_000); // 0.40 * 10^18

    token_info.supply += mint_amount;
    TOKEN_INFO.save(deps.storage, &token_address, &token_info)?;

    Ok(Response::new().add_attribute("action", "mint"))
}

fn transfer(
    deps: DepsMut,
    _info: MessageInfo,
    token_address: String,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let token_address = deps.api.addr_validate(&token_address)?;
    let recipient = deps.api.addr_validate(&recipient)?;

    let fee = amount.u128() / 100; // 1% de taxa
    let amount_after_fee = amount - Uint128::new(fee);

    let fee_msg = BankMsg::Send {
        to_address: token_address.to_string(),
        amount: vec![Coin {
            denom: "utoken".to_string(),
            amount: Uint128::new(fee),
        }],
    };

    let transfer_msg = BankMsg::Send {
        to_address: recipient.to_string(),
        amount: vec![Coin {
            denom: "utoken".to_string(),
            amount: amount_after_fee,
        }],
    };

    Ok(Response::new()
        .add_messages(vec![fee_msg, transfer_msg])
        .add_attribute("action", "transfer"))
}

fn set_mint_enabled(
    deps: DepsMut,
    _info: MessageInfo,
    token_address: String,
    enabled: bool,
) -> Result<Response, ContractError> {
    let token_address = deps.api.addr_validate(&token_address)?;
    let mut token_info = TOKEN_INFO.load(deps.storage, &token_address)?;

    if token_info.immutable_mode {
        return Err(ContractError::ContractLocked {});
    }

    token_info.mint_enabled = enabled;
    TOKEN_INFO.save(deps.storage, &token_address, &token_info)?;

    Ok(Response::new().add_attribute("action", "set_mint_enabled"))
}

fn lock_ownership(
    deps: DepsMut,
    _info: MessageInfo,
    token_address: String,
) -> Result<Response, ContractError> {
    let token_address = deps.api.addr_validate(&token_address)?;
    let mut token_info = TOKEN_INFO.load(deps.storage, &token_address)?;

    token_info.immutable_mode = true;
    TOKEN_INFO.save(deps.storage, &token_address, &token_info)?;

    Ok(Response::new().add_attribute("action", "lock_ownership"))
}
