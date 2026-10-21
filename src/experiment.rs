//! 趋势底仓网格实验：预先固定规则、复用 A 组合法方案，不用最终检验结果选择策略。

use crate::backtest::{self, BacktestMetrics, Replay};
use crate::candles::Candle;
use crate::model::Plan;
use anyhow::Result;
use serde::Serialize;

/// 与 A 组共用时间、投入和静态线位的 B 组实验审计；不表示通过实盘门槛。
#[derive(Clone, Debug, Serialize)]
pub struct Comparison {
    /// 预先固定的规则版本，便于导出复算。
    pub strategy: String,
    /// 只从本段之前的收盘计算出的上涨信号。
    pub uptrend_signal: bool,
    /// 被锁为底仓的完整初始卖格数量；向下取整，不产生零碎订单。
    pub core_slots: usize,
    /// 实际底仓币量，不包含费用储备币，不额外占用资金。
    pub core_base_quantity: f64,
    /// B 组包含底仓浮亏、逐格交易及终止清仓的完整指标。
    pub b: BacktestMetrics,
    /// 与 A、B 同额投入但不设止损的买入持有完整指标，风险敞口不同。
    pub buy_hold: BacktestMetrics,
    /// B 减 A 的净收益百分点；负数明确表示 B 更差。
    pub return_delta_pct_points: f64,
    /// B 净收益减同额持有净收益的百分点，不以此单项判断优劣。
    pub excess_return_pct_points: f64,
}

/// 输入：本段之前的连续历史；返回：SMA20 高于 SMA60 且最后收盘高于 SMA60 的信号。
fn uptrend(prefix: &[Candle]) -> bool {
    if prefix.len() < 60 {
        return false;
    }
    let recent = &prefix[prefix.len() - 60..];
    let slow = recent.iter().map(|c| c.close).sum::<f64>() / 60.0;
    let fast = recent[40..].iter().map(|c| c.close).sum::<f64>() / 20.0;
    fast > slow && recent[59].close > slow
}

/// 输入：A 组合法方案、训练前缀及后续历史；返回：预声明 B 与持有基准，不改变 A 或重选参数。
pub fn compare(
    p: &Plan,
    prefix: &[Candle],
    forward: &[Candle],
    a: &BacktestMetrics,
) -> Result<Comparison> {
    let signal = uptrend(prefix);
    let held = p.grid_prices[..p.grid_count]
        .iter()
        .map(|v| v.parse::<f64>())
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .filter(|price| **price >= p.reference_price)
        .count();
    let core_slots = if signal { held / 2 } else { 0 };
    let b = backtest::evaluate_variant(p, forward, Replay::Core(core_slots))?;
    let buy_hold = backtest::evaluate_variant(p, forward, Replay::Hold)?;
    Ok(Comparison {
        strategy: "trend_core_v1".into(),
        uptrend_signal: signal,
        core_slots,
        core_base_quantity: core_slots as f64 * p.quantity_per_grid.parse::<f64>()?,
        return_delta_pct_points: b.net_return_pct - a.net_return_pct,
        excess_return_pct_points: b.net_return_pct - buy_hold.net_return_pct,
        b,
        buy_hold,
    })
}
