//! 独立细周期成交历史：校验完整覆盖及 OHLC 聚合，不对日线插值制造真实成交数据。

use crate::candles::{Candle, validate_series};
use crate::config::{Settings, interval_ms};
use anyhow::{Context, Result, ensure};
use serde::Serialize;

/// 随最终方案导出的实际细周期样本，供相同输入复算。
#[derive(Clone, Debug, Serialize)]
pub struct ExecutionHistory {
    /// 细周期字符串，与 ATR 周期分开标记。
    pub interval: String,
    /// 原始来源：用户 CSV 路径或公开行情端点。
    pub source: String,
    /// 完整、连续且严格聚合到原周期的已收盘 OHLC。
    pub candles: Vec<Candle>,
}

/// 输入：设置、原周期历史、细周期历史和来源；返回：覆盖与聚合一致的成交快照，否则明确失败。
pub fn validate(
    s: &Settings,
    coarse: &[Candle],
    fine: Vec<Candle>,
    source: String,
) -> Result<ExecutionHistory> {
    let interval = s.replay_interval.as_ref().context("缺少回放周期")?;
    let step = interval_ms(interval)?;
    let ratio = (interval_ms(&s.interval)? / step) as usize;
    validate_series(&fine, interval).context("细周期回放历史不连续或价格无效")?;
    ensure!(
        !coarse.is_empty() && fine.len() == coarse.len() * ratio,
        "细周期回放未完整覆盖 ATR 历史，需 {} 根，实际 {} 根",
        coarse.len() * ratio,
        fine.len()
    );
    for (c, group) in coarse.iter().zip(fine.chunks_exact(ratio)) {
        ensure!(
            group[0].open_time == c.open_time,
            "细周期回放起止时间与 ATR 历史不匹配"
        );
        let values = [
            group[0].open,
            group
                .iter()
                .map(|v| v.high)
                .fold(f64::NEG_INFINITY, f64::max),
            group.iter().map(|v| v.low).fold(f64::INFINITY, f64::min),
            group[ratio - 1].close,
        ];
        ensure!(
            values
                .iter()
                .zip([c.open, c.high, c.low, c.close])
                .all(|(a, b)| (a - b).abs() <= (b.abs() * 1e-10).max(1e-8)),
            "细周期回放 OHLC 聚合与 ATR 历史不一致，开盘毫秒 {}",
            c.open_time
        );
    }
    Ok(ExecutionHistory {
        interval: interval.clone(),
        source,
        candles: fine,
    })
}

/// 输入：原周期的一段待验证历史、可选细周期快照和原周期跨度；返回：同段成交样本，未启用时原样返回。
pub fn window<'a>(
    forward: &'a [Candle],
    execution: Option<&'a ExecutionHistory>,
    coarse_step: u64,
) -> &'a [Candle] {
    let Some(history) = execution else {
        return forward;
    };
    let start = forward[0].open_time;
    let end = forward[forward.len() - 1].open_time + coarse_step;
    let first = history.candles.partition_point(|c| c.open_time < start);
    let last = history.candles.partition_point(|c| c.open_time < end);
    &history.candles[first..last]
}
