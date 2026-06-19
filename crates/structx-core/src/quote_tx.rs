use std::str::FromStr;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use deepbook_client::{SuiObjectInfo, DUSDC_COIN_TYPE, PREDICT_PACKAGE_ID};
use serde::{Deserialize, Serialize};
use sui_sdk_types::{Address, Digest, Identifier, TypeTag};
use sui_transaction_builder::{Function, ObjectInput, TransactionBuilder};
use thiserror::Error;

use crate::payoff::BinaryDirection;
use crate::quote_plan::{QuoteCall, QuotePlan};

#[derive(Debug, Clone, Error)]
pub enum QuoteTxBuildError {
    #[error("invalid Sui type tag `{value}`: {message}")]
    InvalidTypeTag { value: String, message: String },

    #[error("object `{role}` is missing initial shared version")]
    MissingSharedVersion { role: &'static str },

    #[error("invalid Sui address `{value}`: {message}")]
    InvalidAddress { value: String, message: String },

    #[error("failed to BCS encode transaction kind: {0}")]
    Bcs(String),

    #[error("failed to build transaction: {0}")]
    Build(String),
}

#[derive(Debug, Clone, Copy)]
pub struct QuoteObjectRefs<'a> {
    pub predict: &'a SuiObjectInfo,
    pub oracle: &'a SuiObjectInfo,
    pub clock: &'a SuiObjectInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteTxKind {
    pub sender: String,
    pub tx_kind_b64: String,
    pub quote_result_command_indices: Vec<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct MintObjectRefs<'a> {
    pub predict: &'a SuiObjectInfo,
    pub manager: &'a SuiObjectInfo,
    pub oracle: &'a SuiObjectInfo,
    pub clock: &'a SuiObjectInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerPositionRead {
    Binary {
        oracle_id: String,
        expiry_ms: u64,
        strike_raw: u64,
        is_up: bool,
        expected_quantity: u64,
    },
    Range {
        oracle_id: String,
        expiry_ms: u64,
        lower_raw: u64,
        upper_raw: u64,
        expected_quantity: u64,
    },
}

pub fn build_quote_tx_kind(
    plan: &QuotePlan,
    refs: QuoteObjectRefs<'_>,
    sender: &str,
) -> Result<QuoteTxKind, QuoteTxBuildError> {
    let package = parse_address(PREDICT_PACKAGE_ID)?;
    let sender_address = parse_address(sender)?;

    let predict = shared_object_arg("predict", refs.predict, false)?;
    let oracle = shared_object_arg("oracle", refs.oracle, false)?;
    let clock = shared_object_arg("clock", refs.clock, false)?;

    let oracle_id = parse_address(&plan.oracle_id)?;

    let mut tx = TransactionBuilder::new();

    let predict_arg = tx.object(predict);
    let oracle_arg = tx.object(oracle);
    let clock_arg = tx.object(clock);

    let mut quote_result_command_indices = Vec::with_capacity(plan.calls.len());
    let mut command_index = 0usize;

    for call in &plan.calls {
        match call {
            QuoteCall::Binary { direction, expiry_ms, strike, quantity, .. } => {
                let key_fn = match direction {
                    BinaryDirection::Up => "up",
                    BinaryDirection::Down => "down",
                };

                let oracle_id_arg = tx.pure(&oracle_id);
                let expiry_arg = tx.pure(&(*expiry_ms as u64));
                let strike_arg = tx.pure(&strike.raw);

                let key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("market_key"),
                        Identifier::from_static(key_fn),
                    ),
                    vec![oracle_id_arg, expiry_arg, strike_arg],
                );

                command_index += 1;

                let quantity_arg = tx.pure(quantity);

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict"),
                        Identifier::from_static("get_trade_amounts"),
                    ),
                    vec![predict_arg, oracle_arg, key, quantity_arg, clock_arg],
                );

                quote_result_command_indices.push(command_index);
                command_index += 1;
            }
            QuoteCall::Range { expiry_ms, lower, upper, quantity, .. } => {
                let oracle_id_arg = tx.pure(&oracle_id);
                let expiry_arg = tx.pure(&(*expiry_ms as u64));
                let lower_arg = tx.pure(&lower.raw);
                let upper_arg = tx.pure(&upper.raw);

                let key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("range_key"),
                        Identifier::from_static("new"),
                    ),
                    vec![oracle_id_arg, expiry_arg, lower_arg, upper_arg],
                );

                command_index += 1;

                let quantity_arg = tx.pure(quantity);

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict"),
                        Identifier::from_static("get_range_trade_amounts"),
                    ),
                    vec![predict_arg, oracle_arg, key, quantity_arg, clock_arg],
                );

                quote_result_command_indices.push(command_index);
                command_index += 1;
            }
        }
    }

    tx.set_sender(sender_address);
    tx.set_gas_budget(1_000_000);
    tx.set_gas_price(1_000);
    tx.add_gas_objects([ObjectInput::owned(Address::ZERO, 1, Digest::ZERO)]);

    let transaction = tx.try_build().map_err(|err| QuoteTxBuildError::Build(err.to_string()))?;

    let bytes =
        bcs::to_bytes(&transaction.kind).map_err(|err| QuoteTxBuildError::Bcs(err.to_string()))?;

    Ok(QuoteTxKind {
        sender: sender.to_string(),
        tx_kind_b64: BASE64.encode(bytes),
        quote_result_command_indices,
    })
}

pub fn build_create_manager_tx_kind(sender: &str) -> Result<QuoteTxKind, QuoteTxBuildError> {
    let package = parse_address(PREDICT_PACKAGE_ID)?;
    let sender_address = parse_address(sender)?;

    let mut tx = TransactionBuilder::new();

    tx.move_call(
        Function::new(
            package,
            Identifier::from_static("predict"),
            Identifier::from_static("create_manager"),
        ),
        vec![],
    );

    tx.set_sender(sender_address);
    tx.set_gas_budget(1_000_000);
    tx.set_gas_price(1_000);
    tx.add_gas_objects([ObjectInput::owned(Address::ZERO, 1, Digest::ZERO)]);

    let transaction = tx.try_build().map_err(|err| QuoteTxBuildError::Build(err.to_string()))?;

    let bytes =
        bcs::to_bytes(&transaction.kind).map_err(|err| QuoteTxBuildError::Bcs(err.to_string()))?;

    Ok(QuoteTxKind {
        sender: sender.to_string(),
        tx_kind_b64: BASE64.encode(bytes),
        quote_result_command_indices: vec![0],
    })
}

pub fn build_manager_balance_tx_kind(
    manager: &SuiObjectInfo,
    sender: &str,
) -> Result<QuoteTxKind, QuoteTxBuildError> {
    let package = parse_address(PREDICT_PACKAGE_ID)?;
    let sender_address = parse_address(sender)?;

    let manager = shared_object_arg("manager", manager, false)?;

    let mut tx = TransactionBuilder::new();

    let manager_arg = tx.object(manager);

    tx.move_call(
        Function::new(
            package,
            Identifier::from_static("predict_manager"),
            Identifier::from_static("balance"),
        )
        .with_type_args(vec![parse_type_tag(DUSDC_COIN_TYPE)?]),
        vec![manager_arg],
    );

    tx.set_sender(sender_address);
    tx.set_gas_budget(1_000_000);
    tx.set_gas_price(1_000);
    tx.add_gas_objects([ObjectInput::owned(Address::ZERO, 1, Digest::ZERO)]);

    let transaction = tx.try_build().map_err(|err| QuoteTxBuildError::Build(err.to_string()))?;

    let bytes =
        bcs::to_bytes(&transaction.kind).map_err(|err| QuoteTxBuildError::Bcs(err.to_string()))?;

    Ok(QuoteTxKind {
        sender: sender.to_string(),
        tx_kind_b64: BASE64.encode(bytes),
        quote_result_command_indices: vec![0],
    })
}

pub fn build_mint_tx_kind(
    plan: &QuotePlan,
    refs: MintObjectRefs<'_>,
    sender: &str,
) -> Result<QuoteTxKind, QuoteTxBuildError> {
    let package = parse_address(PREDICT_PACKAGE_ID)?;
    let sender_address = parse_address(sender)?;

    let predict = shared_object_arg("predict", refs.predict, true)?;
    let manager = shared_object_arg("manager", refs.manager, true)?;
    let oracle = shared_object_arg("oracle", refs.oracle, false)?;
    let clock = shared_object_arg("clock", refs.clock, false)?;

    let oracle_id = parse_address(&plan.oracle_id)?;

    let mut tx = TransactionBuilder::new();

    let predict_arg = tx.object(predict);
    let manager_arg = tx.object(manager);
    let oracle_arg = tx.object(oracle);
    let clock_arg = tx.object(clock);

    let mut mint_command_indices = Vec::with_capacity(plan.calls.len());
    let mut command_index = 0usize;

    for call in &plan.calls {
        match call {
            QuoteCall::Binary { direction, expiry_ms, strike, quantity, .. } => {
                let key_fn = match direction {
                    BinaryDirection::Up => "up",
                    BinaryDirection::Down => "down",
                };

                let oracle_id_arg = tx.pure(&oracle_id);
                let expiry_arg = tx.pure(&(*expiry_ms as u64));
                let strike_arg = tx.pure(&strike.raw);

                let key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("market_key"),
                        Identifier::from_static(key_fn),
                    ),
                    vec![oracle_id_arg, expiry_arg, strike_arg],
                );

                command_index += 1;

                let quantity_arg = tx.pure(quantity);

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict"),
                        Identifier::from_static("mint"),
                    )
                    .with_type_args(vec![parse_type_tag(DUSDC_COIN_TYPE)?]),
                    vec![predict_arg, manager_arg, oracle_arg, key, quantity_arg, clock_arg],
                );

                mint_command_indices.push(command_index);
                command_index += 1;
            }
            QuoteCall::Range { expiry_ms, lower, upper, quantity, .. } => {
                let oracle_id_arg = tx.pure(&oracle_id);
                let expiry_arg = tx.pure(&(*expiry_ms as u64));
                let lower_arg = tx.pure(&lower.raw);
                let upper_arg = tx.pure(&upper.raw);

                let key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("range_key"),
                        Identifier::from_static("new"),
                    ),
                    vec![oracle_id_arg, expiry_arg, lower_arg, upper_arg],
                );

                command_index += 1;

                let quantity_arg = tx.pure(quantity);

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict"),
                        Identifier::from_static("mint_range"),
                    )
                    .with_type_args(vec![parse_type_tag(DUSDC_COIN_TYPE)?]),
                    vec![predict_arg, manager_arg, oracle_arg, key, quantity_arg, clock_arg],
                );

                mint_command_indices.push(command_index);
                command_index += 1;
            }
        }
    }

    tx.set_sender(sender_address);
    tx.set_gas_budget(1_000_000);
    tx.set_gas_price(1_000);
    tx.add_gas_objects([ObjectInput::owned(Address::ZERO, 1, Digest::ZERO)]);

    let transaction = tx.try_build().map_err(|err| QuoteTxBuildError::Build(err.to_string()))?;

    let bytes =
        bcs::to_bytes(&transaction.kind).map_err(|err| QuoteTxBuildError::Bcs(err.to_string()))?;

    Ok(QuoteTxKind {
        sender: sender.to_string(),
        tx_kind_b64: BASE64.encode(bytes),
        quote_result_command_indices: mint_command_indices,
    })
}

pub fn build_redeem_tx_kind(
    reads: &[ManagerPositionRead],
    refs: MintObjectRefs<'_>,
    sender: &str,
) -> Result<QuoteTxKind, QuoteTxBuildError> {
    let package = parse_address(PREDICT_PACKAGE_ID)?;
    let sender_address = parse_address(sender)?;

    let predict = shared_object_arg("predict", refs.predict, true)?;
    let manager = shared_object_arg("manager", refs.manager, true)?;
    let oracle = shared_object_arg("oracle", refs.oracle, false)?;
    let clock = shared_object_arg("clock", refs.clock, false)?;

    let mut tx = TransactionBuilder::new();

    let predict_arg = tx.object(predict);
    let manager_arg = tx.object(manager);
    let oracle_arg = tx.object(oracle);
    let clock_arg = tx.object(clock);

    let mut redeem_command_indices = Vec::with_capacity(reads.len());
    let mut command_index = 0usize;

    for read in reads {
        match read {
            ManagerPositionRead::Binary {
                oracle_id,
                expiry_ms,
                strike_raw,
                is_up,
                expected_quantity,
            } => {
                let oracle_id = parse_address(oracle_id)?;
                let oracle_id_arg = tx.pure(&oracle_id);
                let expiry_arg = tx.pure(expiry_ms);
                let strike_arg = tx.pure(strike_raw);

                let key_fn = if *is_up { "up" } else { "down" };

                let key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("market_key"),
                        Identifier::from_static(key_fn),
                    ),
                    vec![oracle_id_arg, expiry_arg, strike_arg],
                );

                command_index += 1;

                let quantity_arg = tx.pure(expected_quantity);

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict"),
                        Identifier::from_static("redeem"),
                    )
                    .with_type_args(vec![parse_type_tag(DUSDC_COIN_TYPE)?]),
                    vec![predict_arg, manager_arg, oracle_arg, key, quantity_arg, clock_arg],
                );

                redeem_command_indices.push(command_index);
                command_index += 1;
            }
            ManagerPositionRead::Range {
                oracle_id,
                expiry_ms,
                lower_raw,
                upper_raw,
                expected_quantity,
            } => {
                let oracle_id = parse_address(oracle_id)?;
                let oracle_id_arg = tx.pure(&oracle_id);
                let expiry_arg = tx.pure(expiry_ms);
                let lower_arg = tx.pure(lower_raw);
                let upper_arg = tx.pure(upper_raw);

                let key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("range_key"),
                        Identifier::from_static("new"),
                    ),
                    vec![oracle_id_arg, expiry_arg, lower_arg, upper_arg],
                );

                command_index += 1;

                let quantity_arg = tx.pure(expected_quantity);

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict"),
                        Identifier::from_static("redeem_range"),
                    )
                    .with_type_args(vec![parse_type_tag(DUSDC_COIN_TYPE)?]),
                    vec![predict_arg, manager_arg, oracle_arg, key, quantity_arg, clock_arg],
                );

                redeem_command_indices.push(command_index);
                command_index += 1;
            }
        }
    }

    tx.set_sender(sender_address);
    tx.set_gas_budget(1_000_000);
    tx.set_gas_price(1_000);
    tx.add_gas_objects([ObjectInput::owned(Address::ZERO, 1, Digest::ZERO)]);

    let transaction = tx.try_build().map_err(|err| QuoteTxBuildError::Build(err.to_string()))?;

    let bytes =
        bcs::to_bytes(&transaction.kind).map_err(|err| QuoteTxBuildError::Bcs(err.to_string()))?;

    Ok(QuoteTxKind {
        sender: sender.to_string(),
        tx_kind_b64: BASE64.encode(bytes),
        quote_result_command_indices: redeem_command_indices,
    })
}

pub fn build_redeem_debug_tx_kind(
    reads: &[ManagerPositionRead],
    refs: MintObjectRefs<'_>,
    sender: &str,
) -> Result<QuoteTxKind, QuoteTxBuildError> {
    let package = parse_address(PREDICT_PACKAGE_ID)?;
    let sender_address = parse_address(sender)?;

    let predict = shared_object_arg("predict", refs.predict, true)?;
    let manager = shared_object_arg("manager", refs.manager, true)?;
    let oracle = shared_object_arg("oracle", refs.oracle, false)?;
    let clock = shared_object_arg("clock", refs.clock, false)?;

    let mut tx = TransactionBuilder::new();

    let predict_arg = tx.object(predict);
    let manager_arg = tx.object(manager);
    let oracle_arg = tx.object(oracle);
    let clock_arg = tx.object(clock);

    let mut debug_result_indices = Vec::with_capacity(reads.len());
    let mut command_index = 0usize;

    for read in reads {
        match read {
            ManagerPositionRead::Binary {
                oracle_id,
                expiry_ms,
                strike_raw,
                is_up,
                expected_quantity,
            } => {
                let oracle_id = parse_address(oracle_id)?;
                let key_fn = if *is_up { "up" } else { "down" };

                let pre_oracle_id_arg = tx.pure(&oracle_id);
                let pre_expiry_arg = tx.pure(expiry_ms);
                let pre_strike_arg = tx.pure(strike_raw);

                let pre_key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("market_key"),
                        Identifier::from_static(key_fn),
                    ),
                    vec![pre_oracle_id_arg, pre_expiry_arg, pre_strike_arg],
                );
                command_index += 1;

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict_manager"),
                        Identifier::from_static("position"),
                    ),
                    vec![manager_arg, pre_key],
                );
                debug_result_indices.push(command_index);
                command_index += 1;

                let redeem_oracle_id_arg = tx.pure(&oracle_id);
                let redeem_expiry_arg = tx.pure(expiry_ms);
                let redeem_strike_arg = tx.pure(strike_raw);

                let redeem_key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("market_key"),
                        Identifier::from_static(key_fn),
                    ),
                    vec![redeem_oracle_id_arg, redeem_expiry_arg, redeem_strike_arg],
                );
                command_index += 1;

                let quantity_arg = tx.pure(expected_quantity);

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict"),
                        Identifier::from_static("redeem"),
                    )
                    .with_type_args(vec![parse_type_tag(DUSDC_COIN_TYPE)?]),
                    vec![predict_arg, manager_arg, oracle_arg, redeem_key, quantity_arg, clock_arg],
                );
                command_index += 1;
            }
            ManagerPositionRead::Range {
                oracle_id,
                expiry_ms,
                lower_raw,
                upper_raw,
                expected_quantity,
            } => {
                let oracle_id = parse_address(oracle_id)?;

                let pre_oracle_id_arg = tx.pure(&oracle_id);
                let pre_expiry_arg = tx.pure(expiry_ms);
                let pre_lower_arg = tx.pure(lower_raw);
                let pre_upper_arg = tx.pure(upper_raw);

                let pre_key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("range_key"),
                        Identifier::from_static("new"),
                    ),
                    vec![pre_oracle_id_arg, pre_expiry_arg, pre_lower_arg, pre_upper_arg],
                );
                command_index += 1;

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict_manager"),
                        Identifier::from_static("range_position"),
                    ),
                    vec![manager_arg, pre_key],
                );
                debug_result_indices.push(command_index);
                command_index += 1;

                let redeem_oracle_id_arg = tx.pure(&oracle_id);
                let redeem_expiry_arg = tx.pure(expiry_ms);
                let redeem_lower_arg = tx.pure(lower_raw);
                let redeem_upper_arg = tx.pure(upper_raw);

                let redeem_key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("range_key"),
                        Identifier::from_static("new"),
                    ),
                    vec![
                        redeem_oracle_id_arg,
                        redeem_expiry_arg,
                        redeem_lower_arg,
                        redeem_upper_arg,
                    ],
                );
                command_index += 1;

                let quantity_arg = tx.pure(expected_quantity);

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict"),
                        Identifier::from_static("redeem_range"),
                    )
                    .with_type_args(vec![parse_type_tag(DUSDC_COIN_TYPE)?]),
                    vec![predict_arg, manager_arg, oracle_arg, redeem_key, quantity_arg, clock_arg],
                );
                command_index += 1;
            }
        }
    }

    tx.set_sender(sender_address);
    tx.set_gas_budget(1_000_000);
    tx.set_gas_price(1_000);
    tx.add_gas_objects([ObjectInput::owned(Address::ZERO, 1, Digest::ZERO)]);

    let transaction = tx.try_build().map_err(|err| QuoteTxBuildError::Build(err.to_string()))?;

    let bytes =
        bcs::to_bytes(&transaction.kind).map_err(|err| QuoteTxBuildError::Bcs(err.to_string()))?;

    Ok(QuoteTxKind {
        sender: sender.to_string(),
        tx_kind_b64: BASE64.encode(bytes),
        quote_result_command_indices: debug_result_indices,
    })
}
pub fn build_redeem_precheck_tx_kind(
    reads: &[ManagerPositionRead],
    manager: &SuiObjectInfo,
    sender: &str,
) -> Result<QuoteTxKind, QuoteTxBuildError> {
    let package = parse_address(PREDICT_PACKAGE_ID)?;
    let sender_address = parse_address(sender)?;

    let manager = shared_object_arg("manager", manager, true)?;

    let mut tx = TransactionBuilder::new();
    let manager_arg = tx.object(manager);

    let mut result_indices = Vec::with_capacity(reads.len());
    let mut command_index = 0usize;

    for read in reads {
        match read {
            ManagerPositionRead::Binary { oracle_id, expiry_ms, strike_raw, is_up, .. } => {
                let oracle_id = parse_address(oracle_id)?;
                let oracle_id_arg = tx.pure(&oracle_id);
                let expiry_arg = tx.pure(expiry_ms);
                let strike_arg = tx.pure(strike_raw);

                let key_fn = if *is_up { "up" } else { "down" };

                let key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("market_key"),
                        Identifier::from_static(key_fn),
                    ),
                    vec![oracle_id_arg, expiry_arg, strike_arg],
                );

                command_index += 1;

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict_manager"),
                        Identifier::from_static("position"),
                    ),
                    vec![manager_arg, key],
                );

                result_indices.push(command_index);
                command_index += 1;
            }
            ManagerPositionRead::Range { oracle_id, expiry_ms, lower_raw, upper_raw, .. } => {
                let oracle_id = parse_address(oracle_id)?;
                let oracle_id_arg = tx.pure(&oracle_id);
                let expiry_arg = tx.pure(expiry_ms);
                let lower_arg = tx.pure(lower_raw);
                let upper_arg = tx.pure(upper_raw);

                let key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("range_key"),
                        Identifier::from_static("new"),
                    ),
                    vec![oracle_id_arg, expiry_arg, lower_arg, upper_arg],
                );

                command_index += 1;

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict_manager"),
                        Identifier::from_static("range_position"),
                    ),
                    vec![manager_arg, key],
                );

                result_indices.push(command_index);
                command_index += 1;
            }
        }
    }

    tx.set_sender(sender_address);
    tx.set_gas_budget(1_000_000);
    tx.set_gas_price(1_000);
    tx.add_gas_objects([ObjectInput::owned(Address::ZERO, 1, Digest::ZERO)]);

    let transaction = tx.try_build().map_err(|err| QuoteTxBuildError::Build(err.to_string()))?;

    let bytes =
        bcs::to_bytes(&transaction.kind).map_err(|err| QuoteTxBuildError::Bcs(err.to_string()))?;

    Ok(QuoteTxKind {
        sender: sender.to_string(),
        tx_kind_b64: BASE64.encode(bytes),
        quote_result_command_indices: result_indices,
    })
}

fn shared_object_arg(
    role: &'static str,
    object: &SuiObjectInfo,
    mutable: bool,
) -> Result<ObjectInput, QuoteTxBuildError> {
    let object_id = parse_address(&object.object_id)?;
    let initial_shared_version =
        object.initial_shared_version.ok_or(QuoteTxBuildError::MissingSharedVersion { role })?;

    Ok(ObjectInput::shared(object_id, initial_shared_version, mutable))
}

pub fn build_manager_positions_tx_kind(
    reads: &[ManagerPositionRead],
    manager: &SuiObjectInfo,
    sender: &str,
) -> Result<QuoteTxKind, QuoteTxBuildError> {
    let package = parse_address(PREDICT_PACKAGE_ID)?;
    let sender_address = parse_address(sender)?;
    let manager = shared_object_arg("manager", manager, false)?;

    let mut tx = TransactionBuilder::new();
    let manager_arg = tx.object(manager);

    let mut result_command_indices = Vec::with_capacity(reads.len());
    let mut command_index = 0usize;

    for read in reads {
        match read {
            ManagerPositionRead::Binary { oracle_id, expiry_ms, strike_raw, is_up, .. } => {
                let oracle_id = parse_address(oracle_id)?;
                let oracle_id_arg = tx.pure(&oracle_id);
                let expiry_arg = tx.pure(expiry_ms);
                let strike_arg = tx.pure(strike_raw);

                let key_fn = if *is_up { "up" } else { "down" };

                let key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("market_key"),
                        Identifier::from_static(key_fn),
                    ),
                    vec![oracle_id_arg, expiry_arg, strike_arg],
                );

                command_index += 1;

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict_manager"),
                        Identifier::from_static("position"),
                    ),
                    vec![manager_arg, key],
                );

                result_command_indices.push(command_index);
                command_index += 1;
            }
            ManagerPositionRead::Range { oracle_id, expiry_ms, lower_raw, upper_raw, .. } => {
                let oracle_id = parse_address(oracle_id)?;
                let oracle_id_arg = tx.pure(&oracle_id);
                let expiry_arg = tx.pure(expiry_ms);
                let lower_arg = tx.pure(lower_raw);
                let upper_arg = tx.pure(upper_raw);

                let key = tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("range_key"),
                        Identifier::from_static("new"),
                    ),
                    vec![oracle_id_arg, expiry_arg, lower_arg, upper_arg],
                );

                command_index += 1;

                tx.move_call(
                    Function::new(
                        package,
                        Identifier::from_static("predict_manager"),
                        Identifier::from_static("range_position"),
                    ),
                    vec![manager_arg, key],
                );

                result_command_indices.push(command_index);
                command_index += 1;
            }
        }
    }

    tx.set_sender(sender_address);
    tx.set_gas_budget(1_000_000);
    tx.set_gas_price(1_000);
    tx.add_gas_objects([ObjectInput::owned(Address::ZERO, 1, Digest::ZERO)]);

    let transaction = tx.try_build().map_err(|err| QuoteTxBuildError::Build(err.to_string()))?;

    let bytes =
        bcs::to_bytes(&transaction.kind).map_err(|err| QuoteTxBuildError::Bcs(err.to_string()))?;

    Ok(QuoteTxKind {
        sender: sender.to_string(),
        tx_kind_b64: BASE64.encode(bytes),
        quote_result_command_indices: result_command_indices,
    })
}

fn parse_address(value: &str) -> Result<Address, QuoteTxBuildError> {
    Address::from_str(value).map_err(|err| QuoteTxBuildError::InvalidAddress {
        value: value.to_string(),
        message: err.to_string(),
    })
}

fn parse_type_tag(value: &str) -> Result<TypeTag, QuoteTxBuildError> {
    value.parse::<TypeTag>().map_err(|err| QuoteTxBuildError::InvalidTypeTag {
        value: value.to_string(),
        message: err.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use deepbook_client::{
        AskBounds, LatestPrice, LatestSvi, ObjectOwnerKind, OracleListItem, OracleState,
        StructxMarketStatus, SuiObjectInfo,
    };
    use serde_json::json;

    use super::*;
    use crate::price::DisplayPrice;
    use crate::strike_grid::StrikeGrid;
    use crate::{build_quote_plan, compile_breakout, PriceScale, SelectedMarket, Strike};

    fn shared_info(object_id: &str, type_: &str) -> SuiObjectInfo {
        SuiObjectInfo {
            object_id: object_id.to_string(),
            object_type: Some(type_.to_string()),
            version: Some(1),
            digest: Some("digest".to_string()),
            owner_kind: ObjectOwnerKind::Shared,
            initial_shared_version: Some(1),
            raw: json!({}),
        }
    }

    #[test]
    fn builds_quote_transaction_kind_bytes() {
        let now = Utc.timestamp_millis_opt(1_900_000_000_000).single().expect("valid timestamp");

        let oracle_id = "0x9637934c2b7a4e74f738df4861b103f37744d1495a702caf1eea72c89176934d";

        let market = deepbook_client::MarketSnapshot {
            list_item: OracleListItem {
                oracle_id: Some(oracle_id.to_string()),
                underlying_asset: Some("BTC".to_string()),
                status: Some("active".to_string()),
                expiry_ms: Some((now + Duration::hours(1)).timestamp_millis()),
                extra: Default::default(),
            },
            state: Some(OracleState {
                oracle_id: Some(oracle_id.to_string()),
                underlying_asset: Some("BTC".to_string()),
                status: Some("active".to_string()),
                expiry_ms: Some((now + Duration::hours(1)).timestamp_millis()),
                min_strike: Some(50_000_000_000_000),
                max_strike: Some(90_000_000_000_000),
                tick_size: Some(1_000_000_000),
                raw: json!({}),
            }),
            latest_price: Some(LatestPrice {
                timestamp_ms: Some(now.timestamp_millis()),
                price: Some(63_303_840_000_000.0),
                raw: json!({}),
            }),
            latest_svi: Some(LatestSvi {
                timestamp_ms: Some(now.timestamp_millis()),
                spot: Some(63_303_840_000_000.0),
                forward: Some(63_400_000_000_000.0),
                raw: json!({}),
            }),
            ask_bounds: Some(AskBounds { raw: json!({}) }),
            structx_status: StructxMarketStatus::Usable,
        };

        let selected = SelectedMarket {
            market: &market,
            oracle_id,
            expiry: now + Duration::hours(1),
            spot_raw: 63_303_840_000_000,
            spot_display: DisplayPrice(63_303.84),
            grid: StrikeGrid::new(
                50_000_000_000_000,
                Some(90_000_000_000_000),
                1_000_000_000,
                PriceScale::E9,
            )
            .expect("grid builds"),
        };

        let compiled = compile_breakout(
            Strike { raw: 62_800_000_000_000 },
            Strike { raw: 63_050_000_000_000 },
            Strike { raw: 63_550_000_000_000 },
            Strike { raw: 63_800_000_000_000 },
            1_000,
            400,
        )
        .expect("breakout compiles");

        let plan = build_quote_plan(&selected, &compiled).expect("quote plan builds");

        let refs = QuoteObjectRefs {
            predict: &shared_info(deepbook_client::PREDICT_OBJECT_ID, "predict::Predict"),
            oracle: &shared_info(selected.oracle_id, "oracle::OracleSVI"),
            clock: &shared_info("0x6", "0x2::clock::Clock"),
        };

        let tx = build_quote_tx_kind(
            &plan,
            refs,
            "0x0000000000000000000000000000000000000000000000000000000000000000",
        )
        .expect("tx kind builds");

        assert!(!tx.tx_kind_b64.is_empty());
        assert_eq!(tx.quote_result_command_indices, vec![1, 3, 5, 7]);
    }
}
