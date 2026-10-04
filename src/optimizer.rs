//! 基于三段滚动发展验证与独立尾部检验选择静态网格，保留旧策略及买入持有对照。

use crate::backtest::{self, BacktestMetrics};
use crate::candles::{Candle, wilder_atr};
use crate::config::{Algorithm, Settings};
use crate::model::{Market, Plan};
use anyhow::{Context, Result, ensure};
use serde::Serialize;

/// 一个预先声明的区间/格数策略，None 格数复用旧版最大可行格数搜索。
#[derive(Clone, Copy)]
struct Policy {
    /// 对每段训练前缀 ATR 使用的区间单侧倍数。
    multiplier: f64,
    /// 固定格数，或每段重新按全部约束选择最多可行格数。
    grids: Option<usize>,
}

/// 一段只使用此前数据建仓的验证结果及其旧策略对照。
#[derive(Clone, Debug, Serialize)]
pub struct FoldEvaluation {
    /// 指标预热/训练前缀的最后一根开盘毫秒，不包含本段待验证数据。
    pub train_end_open_ms: u64,
    /// 本段验证首根开盘毫秒。
    pub evaluation_start_open_ms: u64,
    /// 本段验证末根开盘毫秒。
    pub evaluation_end_open_ms: u64,
    /// 本段实际回放根数，未发生未来数据重用。
    pub evaluated_bars: usize,
    /// 当前候选策略的清算净收益、成本和回撤。
    pub candidate: BacktestMetrics,
    /// 原设置对应旧策略的同段回放，历史资金不可行时为 None。
    pub baseline: Option<BacktestMetrics>,
    /// 旧策略不可行时保留实际原因，不能把失败伪造为零收益。
    pub baseline_error: Option<String>,
    /// 与候选投入相同的买入持有本金收益，风险敞口不等同于网格。
    pub buy_hold_return_pct: f64,
}

/// 可随方案导出的完整选参审计，最后 20% 历史从未进入选参评分。
#[derive(Debug, Serialize)]
pub struct OptimizationReport {
    /// 本版可复算的方法标识。
    pub method: String,
    /// 搜索的全部预声明参数组合数。
    pub tested_candidates: usize,
    /// 在全部发展段满足交易、资金和风险约束的组合数。
    pub feasible_candidates: usize,
    /// 仅用发展段选出的区间单侧 ATR 倍数。
    pub selected_range_atr_mult: f64,
    /// 选出的固定格数；None 表示沿用每段最大可行格数政策。
    pub selected_grids: Option<usize>,
    /// 均值收益减最大回撤及收益标准差，再加最差成本压力收益的评分。
    pub development_score: f64,
    /// 相同评分公式下旧策略的发展段评分，不可行时为 None。
    pub baseline_score: Option<f64>,
    /// 三段按时间递增的滚动发展验证，后段可以使用此前已发生数据。
    pub development_folds: Vec<FoldEvaluation>,
    /// 完全未参与候选排名的最后 20% 历史检验。
    pub holdout: FoldEvaluation,
    /// 当前最近 30 根收盘的方向效率，0 为无净方向、1 为单向趋势。
    pub trend_efficiency: f64,
    /// ready 为通过历史门槛，wait 为证据不足；均不是未来收益保证。
    pub recommendation: String,
    /// 对应历史门槛、趋势状态及旧策略对照的中文原因。
    pub reason: String,
}

/// 已在三个发展段可行的候选，保留评分和完整指标而非只存最优数值。
struct Ranked {
    /// 此候选预声明的区间和格数政策。
    policy: Policy,
    /// 用于候选排序的发展段评分。
    score: f64,
    /// 三段独立建仓、各自清算的历史结果。
    folds: Vec<FoldEvaluation>,
}

/// 输入：设置、行情和历史；返回：三个发展段及一个尾部检验的索引界限。
fn splits(s: &Settings, candles: &[Candle]) -> Result<Vec<(usize, usize)>> {
    let warmup = 60.max(s.atr_period + 1);
    let holdout = 20.max(candles.len() / 5);
    let end = candles.len().saturating_sub(holdout);
    ensure!(
        candles.len() >= 120 && end >= warmup + 30,
        "自适应至少需要 120 根已收盘 K 线，且 ATR 预热后须留出三个至少 10 根的发展段和最终检验；实际 {} 根",
        candles.len()
    );
    let width = (end - warmup) / 3;
    Ok(vec![
        (warmup, warmup + width),
        (warmup + width, warmup + width * 2),
        (warmup + width * 2, end),
        (end, candles.len()),
    ])
}

/// 输入：用户设置与订单数上限；返回：包含旧策略政策的预声明候选空间。
fn policies(s: &Settings, market: &Market) -> Vec<Policy> {
    let limit = s.max_grids.min(market.rules.max_orders.unwrap_or(170));
    let counts: Vec<_> = if let Some(n) = s.grids {
        vec![Some(n)]
    } else {
        std::iter::once(None).chain((2..=limit).map(Some)).collect()
    };
    [0.5, 0.75, 1.0, 1.5, 2.0]
        .into_iter()
        .flat_map(|factor| {
            counts.iter().map(move |grids| Policy {
                multiplier: s.range_atr_mult * factor,
                grids: *grids,
            })
        })
        .collect()
}

/// 输入：用户设置与候选；返回：保留资金、风险、成本和停止缓冲的旧规划器设置。
fn policy_settings(s: &Settings, policy: Policy) -> Settings {
    let mut settings = s.clone();
    settings.algorithm = Algorithm::Classic;
    settings.range_atr_mult = policy.multiplier;
    settings.grids = policy.grids;
    settings
}

/// 输入：原行情、训练前缀和设置；返回：仅用此前收盘及 ATR 的历史规划行情。
fn prefix_market(s: &Settings, market: &Market, prefix: &[Candle]) -> Result<Market> {
    let last = prefix.last().context("自适应训练前缀为空")?;
    Ok(Market {
        price: last.close,
        atr: Some(wilder_atr(prefix, s.atr_period)?),
        rules: market.rules.clone(),
        data_source: "walk_forward_prefix".into(),
        atr_source: Some("prefix_wilder_atr".into()),
        closed_candle_count: prefix.len(),
        last_candle_open_ms: Some(last.open_time),
        market_as_of_ms: None,
        history: None,
        candles: Vec::new(),
    })
}

/// 输入：设置、规则快照、历史、时间索引与候选；返回：先用前缀规划再回放后续的一段验证。
fn evaluate_fold(
    s: &Settings,
    market: &Market,
    candles: &[Candle],
    split: (usize, usize),
    policy: Policy,
) -> Result<FoldEvaluation> {
    let (start, end) = split;
    let prefix = &candles[..start];
    let forward = &candles[start..end];
    let settings = policy_settings(s, policy);
    let plan = crate::planner::generate(&settings, prefix_market(s, market, prefix)?)?;
    Ok(FoldEvaluation {
        train_end_open_ms: candles[start - 1].open_time,
        evaluation_start_open_ms: forward[0].open_time,
        evaluation_end_open_ms: forward[forward.len() - 1].open_time,
        evaluated_bars: forward.len(),
        candidate: backtest::evaluate(&plan, forward)?,
        baseline: None,
        baseline_error: None,
        buy_hold_return_pct: backtest::buy_hold_return(&plan, forward[forward.len() - 1].close),
    })
}

/// 输入：三段发展验证；返回：风险与成本压力惩罚后的分数，未使用尾部检验。
fn score(folds: &[FoldEvaluation]) -> f64 {
    let count = folds.len() as f64;
    let mean = folds
        .iter()
        .map(|v| v.candidate.net_return_pct)
        .sum::<f64>()
        / count;
    let variance = folds
        .iter()
        .map(|v| (v.candidate.net_return_pct - mean).powi(2))
        .sum::<f64>()
        / count;
    let drawdown = folds
        .iter()
        .map(|v| v.candidate.max_drawdown_pct)
        .fold(0.0, f64::max);
    let stress = folds
        .iter()
        .map(|v| v.candidate.stress_net_return_pct)
        .fold(f64::INFINITY, f64::min);
    mean - drawdown - variance.sqrt() + stress
}

/// 输入：设置、行情、历史和三个发展段；返回：最高评分可行候选及搜索数量，不读取最终检验价格。
fn select(
    s: &Settings,
    market: &Market,
    candles: &[Candle],
    development: &[(usize, usize)],
) -> Result<(Ranked, usize, usize)> {
    let policies = policies(s, market);
    let total = policies.len();
    let mut feasible = 0;
    let mut best: Option<Ranked> = None;
    for policy in policies {
        let folds = development
            .iter()
            .map(|split| evaluate_fold(s, market, candles, *split, policy))
            .collect::<Result<Vec<_>>>();
        let Ok(folds) = folds else {
            continue;
        };
        feasible += 1;
        let value = score(&folds);
        if best
            .as_ref()
            .is_none_or(|current| value > current.score + 1e-10)
        {
            best = Some(Ranked {
                policy,
                score: value,
                folds,
            });
        }
    }
    Ok((
        best.context("自适应无可行历史候选：资金、风险、成本、停止价或交易精度约束不满足")?,
        total,
        feasible,
    ))
}

/// 输入：待展示段和旧策略同段结果；返回：无；保留实际错误，避免假装基准为零收益。
fn attach_baseline(fold: &mut FoldEvaluation, baseline: Result<FoldEvaluation>) {
    match baseline {
        Ok(value) => fold.baseline = Some(value.candidate),
        Err(error) => fold.baseline_error = Some(format!("{error:#}")),
    }
}

/// 输入：最近历史；返回：(方向效率, 是否强单边下跌)，平盘效率为零。
fn trend(candles: &[Candle]) -> (f64, bool) {
    let recent = &candles[candles.len().saturating_sub(30)..];
    let change = recent.last().unwrap().close - recent.first().unwrap().close;
    let travel = recent
        .windows(2)
        .map(|p| (p[1].close - p[0].close).abs())
        .sum::<f64>();
    let efficiency = if travel > 0.0 {
        change.abs() / travel
    } else {
        0.0
    };
    (efficiency, change < 0.0 && efficiency >= 0.65)
}

/// 输入：选参结果、未参与选参的最终检验与当前趋势；返回：ready/wait 及可显示的中文原因。
fn recommendation(best: &Ranked, holdout: &FoldEvaluation, declining: bool) -> (String, String) {
    let mut reasons = Vec::new();
    if best.score <= 0.0 {
        reasons.push("发展段风险调整评分不为正");
    }
    if best
        .folds
        .iter()
        .filter(|v| v.candidate.net_return_pct > 0.0)
        .count()
        < 2
    {
        reasons.push("三个发展段中盈利段不足两个");
    }
    if holdout.candidate.net_return_pct <= 0.0 {
        reasons.push("最终检验扣费后亏损或持平");
    }
    if holdout.candidate.stress_net_return_pct <= 0.0 {
        reasons.push("最终检验双倍成本后缺少正收益");
    }
    if holdout.candidate.completed_cycles < 1 {
        reasons.push("最终检验没有完成网格成交");
    }
    if holdout
        .baseline
        .as_ref()
        .is_some_and(|b| holdout.candidate.net_return_pct + 1e-9 < b.net_return_pct)
    {
        reasons.push("最终检验收益低于旧算法");
    }
    if declining {
        reasons.push("近期收盘呈强单边下跌");
    }
    if reasons.is_empty() {
        (
            "ready".into(),
            "发展验证与最终检验通过；历史优势不保证未来收益，创建前核对交易所预览。".into(),
        )
    } else {
        ("wait".into(), reasons.join("；"))
    }
}

/// 输入：设置、行情、时间划分和最高分候选；返回：包含旧策略及最终检验的审计报告。
fn audit(
    s: &Settings,
    market: &Market,
    splits: &[(usize, usize)],
    best: &mut Ranked,
    tested: usize,
    feasible: usize,
) -> Result<OptimizationReport> {
    let original = Policy {
        multiplier: s.range_atr_mult,
        grids: s.grids,
    };
    let mut baselines = Vec::new();
    for (fold, split) in best.folds.iter_mut().zip(splits) {
        let baseline = evaluate_fold(s, market, &market.candles, *split, original);
        if let Ok(value) = &baseline {
            baselines.push(value.clone());
        }
        attach_baseline(fold, baseline);
    }
    let mut holdout = evaluate_fold(s, market, &market.candles, splits[3], best.policy)?;
    attach_baseline(
        &mut holdout,
        evaluate_fold(s, market, &market.candles, splits[3], original),
    );
    let (efficiency, declining) = trend(&market.candles);
    let (recommendation, reason) = recommendation(best, &holdout, declining);
    Ok(OptimizationReport {
        method: "anchored_walk_forward_v1".into(),
        tested_candidates: tested,
        feasible_candidates: feasible,
        selected_range_atr_mult: best.policy.multiplier,
        selected_grids: best.policy.grids,
        development_score: best.score,
        baseline_score: (baselines.len() == 3).then(|| score(&baselines)),
        development_folds: best.folds.clone(),
        holdout,
        trend_efficiency: efficiency,
        recommendation,
        reason,
    })
}

/// 输入：已校验自适应设置和当前行情；返回：现价下合法静态方案及可复算审计，未通过则显示观望。
pub fn generate(s: &Settings, market: Market) -> Result<Plan> {
    let splits = splits(s, &market.candles)?;
    let (mut best, tested, feasible) = select(s, &market, &market.candles, &splits[..3])?;
    let report = audit(s, &market, &splits, &mut best, tested, feasible)?;
    let selected = policy_settings(s, best.policy);
    let mut plan = crate::planner::generate(&selected, market)
        .context("自适应选定政策在当前行情下不可行；不会使用最终检验重新调参")?;
    plan.algorithm = Algorithm::Adaptive;
    if report.recommendation == "wait" {
        plan.warnings.insert(
            0,
            format!(
                "自适应建议观望：{}。线位仅供诊断，不建议据此启动。",
                report.reason
            ),
        );
    }
    plan.warnings.push("历史回放假设限价完整成交、建仓按此前收盘价及给定成本执行；K 线内顺序、排队和真实滑点无法由 OHLC 证明。".into());
    plan.optimization = Some(report);
    Ok(plan)
}
