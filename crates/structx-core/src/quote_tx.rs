use std::str::FromStr;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use deepbook_client::{SuiObjectInfo, PREDICT_PACKAGE_ID};
use serde::{Deserialize, Serialize};
use sui_sdk_types::{Address, Digest, Identifier};
use sui_transaction_builder::{Function, ObjectInput, TransactionBuilder};
use thiserror::Error;

use crate::payoff::BinaryDirection;
use crate::quote_plan::{QuoteCall, QuotePlan};

#[derive(Debug, Clone, Error)]
pub enum QuoteTxBuildError {
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

fn parse_address(value: &str) -> Result<Address, QuoteTxBuildError> {
    Address::from_str(value).map_err(|err| QuoteTxBuildError::InvalidAddress {
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
    fn builds_create_manager_transaction_kind_bytes() {
        let tx = build_create_manager_tx_kind(
            "0x0000000000000000000000000000000000000000000000000000000000000000",
        )
        .expect("create-manager tx kind builds");

        assert!(!tx.tx_kind_b64.is_empty());
        assert_eq!(tx.quote_result_command_indices, vec![0]);
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
