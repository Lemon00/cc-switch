//! Cost Calculator - 计算 API 请求成本
//!
//! 使用高精度 Decimal 类型避免浮点数精度问题

use super::parser::TokenUsage;
use rust_decimal::Decimal;
#[cfg(test)]
use std::str::FromStr;

/// 成本明细
#[derive(Debug, Clone, Default)]
pub struct CostBreakdown {
    pub input_cost: Decimal,
    pub output_cost: Decimal,
    pub cache_read_cost: Decimal,
    pub cache_creation_cost: Decimal,
    pub total_cost: Decimal,
}

pub use crate::pricing::ModelPricing;

/// 成本计算器
pub struct CostCalculator;

impl CostCalculator {
    /// 按 app_type 选择输入 token 语义后计算成本。
    ///
    /// Codex/OpenAI Responses 与 Gemini 的输入 token 字段包含 cache read 部分；
    /// Claude/Anthropic 的 input_tokens 已经是 fresh input。
    pub fn calculate_for_app(
        app_type: &str,
        usage: &TokenUsage,
        pricing: &ModelPricing,
        cost_multiplier: Decimal,
    ) -> CostBreakdown {
        Self::try_calculate_for_app(app_type, usage, Some(pricing), cost_multiplier).unwrap_or_else(
            || {
                log::warn!(
                    "Invalid or overflowing usage cost; preserving tokens for later repricing"
                );
                CostBreakdown::default()
            },
        )
    }

    fn calculate_with_cache_semantics(
        usage: &TokenUsage,
        pricing: &ModelPricing,
        cost_multiplier: Decimal,
        input_includes_cache_read: bool,
    ) -> Option<CostBreakdown> {
        if [
            pricing.input_cost_per_million,
            pricing.output_cost_per_million,
            pricing.cache_read_cost_per_million,
            pricing.cache_creation_cost_per_million,
            cost_multiplier,
        ]
        .iter()
        .any(|value| *value < Decimal::ZERO)
        {
            return None;
        }
        let million = Decimal::from(1_000_000);

        // OpenAI/Gemini 风格的 input_tokens 包含缓存读取和写入，需要扣除后再按输入价计费；
        // Claude/Anthropic 风格的 input_tokens 已经是 fresh input，不能再次扣减。
        let billable_input_tokens = if input_includes_cache_read {
            usage
                .input_tokens
                .saturating_sub(usage.cache_read_tokens)
                .saturating_sub(usage.cache_creation_tokens)
        } else {
            usage.input_tokens
        };

        // 各项基础成本（不含倍率）
        let input_cost = Decimal::from(billable_input_tokens)
            .checked_mul(pricing.input_cost_per_million)?
            / million;
        let output_cost = Decimal::from(usage.output_tokens)
            .checked_mul(pricing.output_cost_per_million)?
            / million;
        let cache_read_cost = Decimal::from(usage.cache_read_tokens)
            .checked_mul(pricing.cache_read_cost_per_million)?
            / million;
        // Anthropic 5 分钟缓存写入为输入价 1.25 倍、1 小时写入为 2 倍；定价表的
        // cache_creation 单价对应 5 分钟写入，1 小时写入按其 1.6 倍计。
        let cache_creation_1h_tokens = usage
            .cache_creation_1h_tokens
            .min(usage.cache_creation_tokens);
        let cache_creation_cost =
            (Decimal::from(usage.cache_creation_tokens - cache_creation_1h_tokens)
                + Decimal::from(cache_creation_1h_tokens) * Decimal::new(16, 1))
            .checked_mul(pricing.cache_creation_cost_per_million)?
                / million;

        // 总成本 = 各项基础成本之和 × 倍率
        let base_total = input_cost
            .checked_add(output_cost)?
            .checked_add(cache_read_cost)?
            .checked_add(cache_creation_cost)?;
        let total_cost = base_total.checked_mul(cost_multiplier)?;

        Some(CostBreakdown {
            input_cost,
            output_cost,
            cache_read_cost,
            cache_creation_cost,
            total_cost,
        })
    }

    pub fn try_calculate_for_app(
        app_type: &str,
        usage: &TokenUsage,
        pricing: Option<&ModelPricing>,
        cost_multiplier: Decimal,
    ) -> Option<CostBreakdown> {
        Self::calculate_with_cache_semantics(
            usage,
            pricing?,
            cost_multiplier,
            crate::database::sql_helpers::is_cache_inclusive_app(app_type),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_or_overflowing_prices_do_not_panic_or_produce_negative_costs() {
        let usage = TokenUsage {
            input_tokens: u32::MAX,
            ..Default::default()
        };
        for input in [Decimal::MAX, Decimal::NEGATIVE_ONE] {
            let pricing = ModelPricing {
                input_cost_per_million: input,
                output_cost_per_million: Decimal::ZERO,
                cache_read_cost_per_million: Decimal::ZERO,
                cache_creation_cost_per_million: Decimal::ZERO,
            };
            assert!(CostCalculator::try_calculate_for_app(
                "claude",
                &usage,
                Some(&pricing),
                Decimal::ONE
            )
            .is_none());
            assert_eq!(
                CostCalculator::calculate_for_app("claude", &usage, &pricing, Decimal::ONE)
                    .total_cost,
                Decimal::ZERO
            );
        }
        let pricing = ModelPricing::from_strings("1", "0", "0", "0").unwrap();
        assert!(CostCalculator::try_calculate_for_app(
            "claude",
            &usage,
            Some(&pricing),
            Decimal::MAX
        )
        .is_none());
    }

    #[test]
    fn test_cost_calculation() {
        let usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 200,
            cache_creation_tokens: 100,
            cache_creation_1h_tokens: 0,
            model: None,
            message_id: None,
        };

        let pricing = ModelPricing::from_strings("3.0", "15.0", "0.3", "3.75").unwrap();
        let multiplier = Decimal::from_str("1.0").unwrap();

        let cost = CostCalculator::calculate_for_app("claude", &usage, &pricing, multiplier);

        // Claude/Anthropic 语义：input_tokens 已经不含 cache_read_tokens
        // input: 1000 * 3.0 / 1M = 0.003
        assert_eq!(cost.input_cost, Decimal::from_str("0.003").unwrap());
        // output: 500 * 15.0 / 1M = 0.0075
        assert_eq!(cost.output_cost, Decimal::from_str("0.0075").unwrap());
        // cache_read: 200 * 0.3 / 1M = 0.00006
        assert_eq!(cost.cache_read_cost, Decimal::from_str("0.00006").unwrap());
        // cache_creation: 100 * 3.75 / 1M = 0.000375
        assert_eq!(
            cost.cache_creation_cost,
            Decimal::from_str("0.000375").unwrap()
        );
        // total: 0.003 + 0.0075 + 0.00006 + 0.000375 = 0.010935
        assert_eq!(cost.total_cost, Decimal::from_str("0.010935").unwrap());
    }

    #[test]
    fn test_claude_1h_cache_writes_billed_at_1h_rate() {
        // Claude Opus 4.8：5m 缓存写入 $6.25/M，1h 缓存写入 $10/M（输入价的 2 倍）
        let pricing = ModelPricing::from_strings("5", "25", "0.50", "6.25").unwrap();
        let usage = serde_json::json!({
            "input_tokens": 2,
            "output_tokens": 1,
            "cache_read_input_tokens": 0,
            "cache_creation_input_tokens": 1000,
            "cache_creation": {
                "ephemeral_5m_input_tokens": 400,
                "ephemeral_1h_input_tokens": 600
            }
        });
        let response = serde_json::json!({ "model": "claude-opus-4-8", "usage": usage });
        let events = [
            serde_json::json!({
                "type": "message_start",
                "message": { "model": "claude-opus-4-8", "usage": usage }
            }),
            serde_json::json!({ "type": "message_delta", "usage": { "output_tokens": 5 } }),
        ];

        for parsed in [
            TokenUsage::from_claude_response(&response),
            TokenUsage::from_claude_stream_events(&events),
        ] {
            let cost = CostCalculator::calculate_for_app(
                "claude",
                &parsed.unwrap(),
                &pricing,
                Decimal::ONE,
            );
            // 400 × 6.25 / 1M + 600 × 10 / 1M = 0.0085
            assert_eq!(
                cost.cache_creation_cost,
                Decimal::from_str("0.0085").unwrap()
            );
        }
    }

    #[test]
    fn test_cost_calculation_for_cache_inclusive_app() {
        let usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 200,
            cache_creation_tokens: 100,
            cache_creation_1h_tokens: 0,
            model: None,
            message_id: None,
        };

        let pricing = ModelPricing::from_strings("3.0", "15.0", "0.3", "3.75").unwrap();
        let multiplier = Decimal::from_str("1.0").unwrap();

        let cost = CostCalculator::calculate_for_app("codex", &usage, &pricing, multiplier);

        // Codex/OpenAI 语义：input_tokens 包含 cache read/write，两桶都需扣除。
        assert_eq!(cost.input_cost, Decimal::from_str("0.0021").unwrap());
        assert_eq!(cost.output_cost, Decimal::from_str("0.0075").unwrap());
        assert_eq!(cost.cache_read_cost, Decimal::from_str("0.00006").unwrap());
        assert_eq!(
            cost.cache_creation_cost,
            Decimal::from_str("0.000375").unwrap()
        );
        assert_eq!(cost.total_cost, Decimal::from_str("0.010035").unwrap());
    }

    #[test]
    fn grokbuild_does_not_double_bill_cached_input() {
        let usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 0,
            cache_read_tokens: 600,
            cache_creation_tokens: 0,
            cache_creation_1h_tokens: 0,
            model: None,
            message_id: None,
        };
        let pricing = ModelPricing::from_strings("10", "0", "1", "0").unwrap();

        let cost = CostCalculator::calculate_for_app("grokbuild", &usage, &pricing, Decimal::ONE);

        assert_eq!(cost.input_cost, Decimal::from_str("0.004").unwrap());
        assert_eq!(cost.cache_read_cost, Decimal::from_str("0.0006").unwrap());
        assert_eq!(cost.total_cost, Decimal::from_str("0.0046").unwrap());
    }

    #[test]
    fn test_cost_multiplier() {
        let usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cache_creation_1h_tokens: 0,
            model: None,
            message_id: None,
        };

        let pricing = ModelPricing::from_strings("3.0", "15.0", "0", "0").unwrap();
        let multiplier = Decimal::from_str("1.5").unwrap();

        let cost = CostCalculator::calculate_for_app("claude", &usage, &pricing, multiplier);

        // input_cost: 基础价格（不含倍率）= 1000 * 3.0 / 1M = 0.003
        assert_eq!(cost.input_cost, Decimal::from_str("0.003").unwrap());
        // total_cost: 基础价格 × 倍率 = 0.003 * 1.5 = 0.0045
        assert_eq!(cost.total_cost, Decimal::from_str("0.0045").unwrap());
    }

    #[test]
    fn test_decimal_precision() {
        let usage = TokenUsage {
            input_tokens: 1,
            output_tokens: 1,
            cache_read_tokens: 1,
            cache_creation_tokens: 1,
            cache_creation_1h_tokens: 0,
            model: None,
            message_id: None,
        };

        let pricing = ModelPricing::from_strings("0.075", "0.3", "0.01875", "0.075").unwrap();
        let multiplier = Decimal::from_str("1.0").unwrap();

        let cost = CostCalculator::calculate_for_app("claude", &usage, &pricing, multiplier);

        // 验证高精度计算
        assert!(cost.total_cost > Decimal::ZERO);
        assert!(cost.total_cost.to_string().len() > 2); // 确保保留了小数位
    }
}
