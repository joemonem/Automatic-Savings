use std::env;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_binary, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
    Uint128,
};

use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{BalanceResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{State, STATE};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:automatic-savings";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAIN_ADDRESS: &str = "wasm1pze5wsf0dg0fa4ysnttugn0m22ssf3t4a9yz3h";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        owner: deps.api.addr_validate(MAIN_ADDRESS)?,
        amount_received: info.funds.clone(),
        savings_rate: msg.savings_rate,
    };
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("rate", "15"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Transfer {
            received_funds,
            savings_rate,
        } => execute_transfer(deps, info, received_funds, savings_rate),
        ExecuteMsg::Flush {} => execute_flush(deps, env, info),
    }
}

pub fn execute_transfer(
    deps: DepsMut,
    info: MessageInfo,
    received_funds: Coin,
    savings_rate: u8,
) -> Result<Response, ContractError> {
    STATE.load(deps.storage)?;

    // valid saving amount
    if savings_rate > 100 || savings_rate == 0 {
        return Err(ContractError::InvalidSavingsRate {});
    }
    // only owner can transfer
    if String::from(info.sender) != String::from(MAIN_ADDRESS) {
        return Err(ContractError::Unauthorized {});
    }
    //amount received has to be greater than 0
    if received_funds.amount <= Uint128::from(0 as u32) {
        return Err(ContractError::EmptyTransfer {});
    }

    let saved = u128::from(100 - savings_rate);

    let send_amount = (saved * u128::from(received_funds.amount)) / u128::from(100 as u32);
    let send = coins(send_amount, received_funds.denom);

    Ok(Response::new()
        .add_message(BankMsg::Send {
            to_address: MAIN_ADDRESS.to_string(),
            amount: send,
        })
        .add_attribute("action", "transfer"))
}
pub fn execute_flush(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    // only owner can flush
    if String::from(info.sender) != MAIN_ADDRESS {
        return Err(ContractError::Unauthorized {});
    }

    let balance = deps.querier.query_all_balances(&env.contract.address)?;
    // can't flush empty balance
    if balance.is_empty() {
        return Err(ContractError::EmptyBalance {});
    }
    Ok(Response::new()
        .add_message(BankMsg::Send {
            to_address: MAIN_ADDRESS.to_string(),
            amount: balance,
        })
        .add_attribute("action", "flush"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetBalance {} => to_binary(&query_balance(deps, env)?),
    }
}

fn query_balance(deps: Deps, env: Env) -> StdResult<BalanceResponse> {
    let balance = deps.querier.query_all_balances(&env.contract.address)?;
    Ok(BalanceResponse { balance })
}

#[cfg(test)]
mod tests {

    use std::io::Read;

    use crate::state::{config, config_read};

    use super::*;
    use cosmwasm_std::{
        coin,
        testing::{mock_dependencies, mock_env, mock_info},
        Addr, CosmosMsg, Storage, SubMsg,
    };

    #[test]
    fn try_instantiate() {
        let mut deps = mock_dependencies();
        let info = mock_info("anyone", &coins(2, "BTC"));

        let msg = InstantiateMsg { savings_rate: 15 };
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(("action", "instantiate"), res.attributes[0]);

        let state = STATE.load(&deps.storage);

        assert_eq!(
            state,
            Ok(State {
                owner: Addr::unchecked(MAIN_ADDRESS),
                amount_received: coins(2, "BTC"),
                savings_rate: 15,
            })
        );
    }

    #[test]
    fn try_transfer() {
        let mut deps = mock_dependencies();
        let info = mock_info("anyone", &[]);

        instantiate(
            deps.as_mut(),
            mock_env(),
            info,
            InstantiateMsg { savings_rate: 15 },
        )
        .unwrap();

        // only owner can transfer
        let info = mock_info("anyone", &coins(1, "BTC"));
        let msg = ExecuteMsg::Transfer {
            received_funds: info.funds[0].clone(),
            savings_rate: 15,
        };
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
        // can't receive empty funds
        let info = mock_info(&MAIN_ADDRESS, &coins(0, "BTC"));
        let msg = ExecuteMsg::Transfer {
            received_funds: info.funds[0].clone(),
            savings_rate: 15,
        };
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::EmptyTransfer {});

        // savings must be above 0 and less than 100
        let info = mock_info(&MAIN_ADDRESS, &coins(2, "BTC"));
        let msg = ExecuteMsg::Transfer {
            received_funds: info.funds[0].clone(),
            savings_rate: 101,
        };

        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidSavingsRate {});

        // works
        let info = mock_info(&MAIN_ADDRESS, &coins(8500, "UST"));
        let msg = ExecuteMsg::Transfer {
            received_funds: info.funds[0].clone(),
            savings_rate: 15,
        };

        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.messages.len());
        assert_eq!(
            res.messages[0],
            SubMsg::new(BankMsg::Send {
                to_address: MAIN_ADDRESS.to_string(),
                amount: coins(7225, "UST"),
            }),
        );
    }

    #[test]
    fn try_flush() {
        let mut deps = mock_dependencies();
        let info = mock_info("anyone", &coins(1000, "ATOM"));

        instantiate(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            InstantiateMsg { savings_rate: 15 },
        )
        .unwrap();

        // only owner can flush
        let info = mock_info("anyone", &[]);

        let err = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Flush {}).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        // can't flush an empty balance, set empty balance before instantiation
        let env = mock_env();
        let info = mock_info(&MAIN_ADDRESS, &[]);

        let err = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Flush {}).unwrap_err();
        assert_eq!(err, ContractError::EmptyBalance {});

        // works
        let env = mock_env();
        let info = mock_info(&MAIN_ADDRESS, &[]);
        deps.querier
            .update_balance(&env.contract.address, coins(2000, "ETH"));
        let res = execute(deps.as_mut(), mock_env(), info, ExecuteMsg::Flush {}).unwrap();
        assert_eq!(1, res.messages.len());
    }
}
