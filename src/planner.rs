//! 网格核心：先定线位，再按双边成本、统一数量、账户风险及交易精度搜索可行格数。

use crate::config::{Algorithm, GridMode, RangeMode, Settings, positive};
use crate::model::{Market, Plan};
use crate::precision::{Direction, Step};
use anyhow::{Context, Result, bail, ensure};

/// 已按价格步长校验的区间与外部触发价；先生成一次，再供所有格数候选复用。
struct Range {
    /// 按 tickSize 向下取整的区间下限，单位 USDT。
    lower: f64,
    /// 按 tickSize 向上取整的区间上限，单位 USDT。
    upper: f64,
    /// 严格低于下限的止损触发价，单位 USDT。
    stop: f64,
    /// 严格高于上限的机器人停止价，单位 USDT。
    take: f64,
}

/// 某一个格数的可行性计算结果；只有资金、收益、精度和风险都满足才转换为 Plan。
struct Candidate {
    /// 从下限到上限的 N+1 个严格递增价格数值。
    prices: Vec<f64>,
    /// 每格统一基础币数量，已按 stepSize 向下取整。
    qty: f64,
    /// 满足全部约束后的建议投入，单位 USDT；向上取整到分。
    investment: f64,
    /// 预留基础币在现价下的购买成本，单位 USDT；包含在总投入中。
    reserve: f64,
    /// 启动时需购买的基础币估算，含高于现价的卖格及费用预留币。
    initial_base: f64,
    /// 单边下跌、完整成交且按 SL 卖出的情景亏损，单位 USDT。
    stop_loss: f64,
    /// 跌穿 SL 后再下跌 stress_pct 时的情景亏损，可能超过预算。
    stress_loss: f64,
    /// 压力情景假设的实际卖出价格，单位 USDT。
    stress_exit: f64,
    /// 取整后所有相邻格中最低的毛收益百分比。
    gross: f64,
    /// 取整后所有相邻格中最低的双边扣费/成本净收益百分比。
    net: f64,
}

/// 输入：设置和已读取行情；返回：满足资金、风险、成本与精度约束的最多格数方案。
pub fn generate(settings: &Settings, market: Market) -> Result<Plan> {
    positive("参考现价", market.price)?;
    let price_step = Step::new(&market.rules.tick_size).context("价格精度错误")?; // 交易所价格 tickSize 的解析结果，负责线位取整和输出。
    let qty_step = Step::new(&market.rules.step_size).context("数量精度错误")?; // 交易所数量 stepSize 的解析结果，负责统一基础币数量取整。
    let range = make_range(settings, &market, &price_step)?; // 上下限及区间外 SL/TP；所有候选使用同一组线位。
    market
        .rules
        .validate_prices(&[range.lower, range.upper, range.stop, range.take])?;
    let limit = settings // 用户搜索上限与交易所最大订单数的较小值。
        .max_grids
        .min(market.rules.max_orders.unwrap_or(170));
    // 固定格数只试一次；自动模式按格数从大到小搜索。
    let counts: Vec<_> = if let Some(n) = settings.grids {
        ensure!(n <= limit, "固定格数超过交易所订单数量限制");
        vec![n]
    } else {
        (2..=limit).rev().collect()
    };
    let mut last_error = "交易所允许的订单数量少于两格".to_string(); // 保留最后失败原因，全部不可行时给出具体约束提示。
    for n in counts {
        // n 是本轮候选段数；首个可行值就是搜索顺序中的最大值。
        match candidate(settings, &market, &range, n, &price_step, &qty_step) {
            Ok(candidate) => {
                return build_plan(settings, market, range, candidate, (&price_step, &qty_step));
            }
            Err(error) => last_error = format!("{n} 格：{error}"),
        }
    }
    bail!(
        "无可行网格方案。{last_error}。请检查区间、资金、风险预算、单格金额与成本门槛；不要强制凑格数"
    )
}

/// 输入：设置、参考价和价格步长；返回：区间外止损止盈，外向取整保留缓冲。
fn make_range(s: &Settings, market: &Market, tick: &Step) -> Result<Range> {
    let price = market.price; // 参考现价或当前生成价位，单位 USDT/基础币。
    let (lower, upper) = range_limits(s, market)?; // 模型原始边界，此时尚未按 tickSize 取整。
    positive("区间下限", lower)?;
    positive("区间上限", upper)?;
    ensure!(
        lower < price && price < upper,
        "需要 lower < 参考现价 < upper；区间外启动不属于此资金模型"
    );
    // 模型触发价分别留在下限外、上限外，不把区间端点当成 SL/TP。
    let lower = tick.quantize(lower, Direction::Down)?; // 区间下限，按价格步长向下取整以向外保留覆盖。
    let upper = tick.quantize(upper, Direction::Up)?; // 区间上限，按价格步长向上取整以向外保留覆盖。
    let (stop, take) = if s.mode == RangeMode::Atr {
        let atr = market.atr.context("缺少 ATR")?; // 所选周期的绝对波幅，单位 USDT；倍数不是概率置信度。
        (lower - s.stop_atr_mult * atr, upper + s.take_atr_mult * atr)
    } else {
        (
            lower * (1.0 - s.outside_pct / 100.0),
            upper * (1.0 + s.outside_pct / 100.0),
        )
    };
    let stop = s.stop_loss.unwrap_or(stop); // 优先使用手填止损，否则使用模型的区间外缓冲价。
    let take = s.take_profit.unwrap_or(take); // 优先使用手填停止价，否则使用模型的区间外缓冲价。
    positive("止损价", stop)?;
    positive("止盈价", take)?;
    ensure!(
        stop < lower && take > upper,
        "止损必须低于区间下限，止盈必须高于区间上限"
    );
    // 上下限及区间外 SL/TP；所有候选使用同一组线位。
    let range = Range {
        lower,
        upper,
        stop: tick.quantize(stop, Direction::Down)?,
        take: tick.quantize(take, Direction::Up)?,
    };
    ensure!(
        range.stop > 0.0 && range.stop < range.lower && range.upper < range.take,
        "价格步长过大，无法生成合法区间和止盈止损"
    );
    Ok(range)
}

/// 输入：设置和行情；返回：所选模型的原始区间，不在此阶段做交易精度取整。
fn range_limits(s: &Settings, market: &Market) -> Result<(f64, f64)> {
    let price = market.price; // 参考现价或当前生成价位，单位 USDT/基础币。
    Ok(match s.mode {
        RangeMode::Percent => (
            price * (1.0 - s.down_pct / 100.0),
            price * (1.0 + s.up_pct / 100.0),
        ),
        RangeMode::Atr => {
            let atr = market.atr.context("缺少 ATR")?; // 所选周期的绝对波幅，单位 USDT；倍数不是概率置信度。
            (
                price - s.range_atr_mult * atr,
                price + s.range_atr_mult * atr,
            )
        }
        RangeMode::Manual => (s.lower.context("缺少下限")?, s.upper.context("缺少上限")?),
    })
}

/// 输入：区间、格数、价格步长与网格类型；返回：N+1 个递增价格，任何重叠价位都拒绝。
fn prices(range: &Range, n: usize, tick: &Step, mode: GridMode) -> Result<Vec<f64>> {
    let ratio = (range.upper / range.lower).powf(1.0 / n as f64); // 等比价格倍数 (上限/下限)^(1/N)，不是扣费后的收益。
    let mut prices = Vec::with_capacity(n + 1); // N+1 个价格点；N 个网格的资金分配使用前 N 个点。
    for i in 0..=n {
        // i 为从 0 到 N 的价格点序号；最后一点必须保留精确上限。
        let raw = match mode {
            GridMode::Geometric => range.lower * ratio.powf(i as f64),
            GridMode::Arithmetic => range.lower + (range.upper - range.lower) * i as f64 / n as f64,
        };
        let price = if i == n {
            range.upper
        } else {
            tick.quantize(raw, Direction::Down)?
        };
        prices.push(price);
    }
    ensure!(
        prices.windows(2).all(|p| p[0] < p[1]),
        "价格精度导致网格价位重叠，请减少格数或扩大区间"
    );
    Ok(prices)
}

/// 输入：已校验价位和价格步长；返回：完整十进制字符串数组，任何转换错误使报告失败。
fn format_prices(prices: &[f64], tick: &Step) -> Result<Vec<String>> {
    prices.iter().map(|price| tick.text(*price)).collect()
}

/// 输入：s 为成本/资金设置，market 为快照，range 为线位，n 为段数，tick/step 为价格/数量步长。
/// 返回：当前 n 格候选，或净收益、单笔金额、账户风险等约束错误。
/// 先检验取整后的最差一格，再模拟单边下跌全库存清仓；费用预留币也会跌价，不能忽略。
fn candidate(
    s: &Settings,
    market: &Market,
    range: &Range,
    n: usize,
    tick: &Step,
    step: &Step,
) -> Result<Candidate> {
    let prices = prices(range, n, tick, s.grid_mode)?; // 两种类型共用全部资金、风险、成本及精度校验。
    let gross = prices // 逐格比较后取最小毛收益，避免价格取整使某格过密。
        .windows(2)
        .map(|p| (p[1] / p[0] - 1.0) * 100.0)
        .fold(f64::INFINITY, f64::min);
    let cost = (s.fee_pct + s.slippage_pct) / 100.0; // 单边手续费与滑点假设的比例，百分比仅在此除以 100。
    let net = prices // 买入按 1+cost、卖出按 1-cost 扣成本后的最差一格收益。
        .windows(2)
        .map(|p| ((1.0 - cost) * p[1] / p[0] - 1.0 - cost) * 100.0)
        .fold(f64::INFINITY, f64::min);
    ensure!(
        net + 1e-10 >= s.min_net_pct,
        "取整后最差一格净收益 {net:.4}% 低于门槛 {:.4}%",
        s.min_net_pct
    );
    let (qty, investment) = size(s, market, range, &prices, step)?; // 同时满足资金与风险预算的数量/投入。
    let entry_cost: f64 = prices[..n] // 全部 N 格最终买齐所需的成本；高于现价的格按现价启动购币。
        .iter()
        .map(|p| p.min(market.price) * (1.0 + cost) * qty)
        .sum();
    let reserve = investment - entry_cost; // 总投入扣除网格购买成本后留下的费用预留成本。
    let reserve_base = reserve / (market.price * (1.0 + cost)); // 按现价及买入成本折算的预留基础币数量。
    let total_base = n as f64 * qty + reserve_base; // 单边跌至下限后 N 格基础币加预留币的总库存。
    let stop_loss = investment - total_base * range.stop * (1.0 - cost); // 全库存按 SL 扣卖出成本清仓后的情景亏损，单位 USDT。
    let stress_exit = range.stop * (1.0 - s.stress_pct / 100.0); // 假设跌穿止损后再跌 stress_pct 的实际卖出价。
    let stress_loss = investment - total_base * stress_exit * (1.0 - cost); // 按压力卖出价清仓的情景亏损，用于暴露超预算可能。
    ensure!(stop_loss <= risk_budget(s) + 1e-9, "数量取整后超过风险预算");
    let initial_slots = prices[..n].iter().filter(|p| **p >= market.price).count(); // 启动时价格点不低于现价的卖格数，需要提前买入相应基础币。
    Ok(Candidate {
        prices,
        qty,
        investment,
        reserve,
        initial_base: initial_slots as f64 * qty + reserve_base,
        stop_loss,
        stress_loss,
        stress_exit,
        gross,
        net,
    })
}

/// 输入：s 为账户/成本设置，market 为报价与规则，range 为 SL/区间，prices 为 N+1 个价位，step 为数量步长。
/// 返回：(每格基础币数量, 总投入 USDT)；不可行时返回错误。
/// 按数量线性缩放成本与止损亏损，同时求出资金上限、风险上限及交易所上限，取最小值。
fn size(
    s: &Settings,
    market: &Market,
    range: &Range,
    prices: &[f64],
    step: &Step,
) -> Result<(f64, f64)> {
    let n = prices.len() - 1; // 价格点数减一，得到网格段数，避免按 N+1 误分配资金。
    let cost = (s.fee_pct + s.slippage_pct) / 100.0; // 单边手续费与滑点假设的比例，百分比仅在此除以 100。
    let reserve_rate = s.reserve_pct / 100.0; // 费用预留占总投入的比例，而非额外增加一笔预算。
    let entry_sum: f64 = prices[..n] // 每格数量设为 1 基础币时，N 格购买成本的总和。
        .iter()
        .map(|p| p.min(market.price) * (1.0 + cost))
        .sum();
    let funding_unit = entry_sum / (1.0 - reserve_rate); // 每格数量为 1 时含费用预留的总资金：购买成本/(1-预留比例)。
    let reserve_unit = funding_unit - entry_sum; // 每格数量为 1 时的费用预留成本，也纳入止损风险。
    let reserve_loss_rate = 1.0 - range.stop * (1.0 - cost) / (market.price * (1.0 + cost)); // 预留币从含买费现价跌到扣卖费 SL 的亏损比例。
    let loss_unit = // 每格数量为 1 时，网格库存与预留币合计的止损情景亏损。
        entry_sum - n as f64 * range.stop * (1.0 - cost) + reserve_unit * reserve_loss_rate;
    let budget = risk_budget(s); // 账户资产乘风险百分比的 USDT 预算，只约束给定止损情景。
    ensure!(
        budget > 0.01 && loss_unit.is_finite() && loss_unit > 0.0,
        "风险预算过低或风险计算无效"
    );
    let cap = (s.capital * 100.0).floor() / 100.0; // 投入上限向下取整到分，避免资金预算因小数尾差超支。
    let maximum = (cap / funding_unit) // 资金、风险、最高单笔数量及金额共同允许的每格数量上限。
        .min((budget - 0.01) / loss_unit)
        .min(market.rules.max_qty.unwrap_or(f64::INFINITY))
        .min(market.rules.max_notional.unwrap_or(f64::INFINITY) / range.upper);
    let qty = step.quantize(maximum, Direction::Down)?; // 数量向下取整到 stepSize，确保资金及风险不会被向上取整放大。
    ensure!(
        qty > 0.0 && qty >= market.rules.min_qty,
        "数量取整后低于 LOT_SIZE 最低数量"
    );
    let minimum = s.min_order_usdt.max(market.rules.min_notional); // 用户最低单笔金额与交易所最低金额的较大值。
    ensure!(
        qty * range.lower + 1e-9 >= minimum,
        "最低网格订单仅 {:.4} USDT，需要至少 {minimum:.4} USDT；资金或风险预算不足",
        qty * range.lower
    );
    let investment = Step::new("0.01")?.quantize(qty * funding_unit, Direction::Up)?; // 数量确定后总资金向上取整到 USDT 分，再检查仍不超过本金。
    ensure!(investment <= cap + 1e-9, "投入金额取整后超出可用本金");
    Ok((qty, investment))
}

/// 输入：账户设置；返回：本策略的账户级止损情景预算，单位 USDT。
fn risk_budget(s: &Settings) -> f64 {
    s.equity.unwrap_or(s.capital) * s.risk_pct / 100.0
}

/// 输入：设置；返回：固定格数或经典最多可行格数的解释，避免把结果误认为硬编码上限。
fn grid_count_reason(s: &Settings) -> String {
    if let Some(n) = s.grids {
        format!("用户指定 {n} 格；仍需通过单格成本、订单金额、精度和风险校验。")
    } else {
        "经典算法在成本、单笔金额、精度和风险约束下取最多可行格数；增加资金不会扩大同一区间每格的比例价差。".into()
    }
}

/// 输入：设置、行情、区间、候选及价格/数量步长；返回：可导出、可填写币安的完整方案。
fn build_plan(
    s: &Settings,
    market: Market,
    range: Range,
    c: Candidate,
    steps: (&Step, &Step),
) -> Result<Plan> {
    Ok(Plan {
        warnings: warnings(s, &market),
        symbol: s.symbol.clone(),
        mode: s.grid_mode,
        range_model: format!("{:?}", s.mode).to_lowercase(),
        algorithm: Algorithm::Classic,
        optimization: None,
        capital_limit_usdt: s.capital,
        account_equity_usdt: s.equity.unwrap_or(s.capital),
        investment_usdt: c.investment,
        unallocated_usdt: s.capital - c.investment,
        reference_price: market.price,
        lower_price: steps.0.text(range.lower)?,
        upper_price: steps.0.text(range.upper)?,
        stop_loss: steps.0.text(range.stop)?,
        take_profit: steps.0.text(range.take)?,
        grid_count: c.prices.len() - 1,
        grid_count_reason: grid_count_reason(s),
        quantity_per_grid: steps.1.text(c.qty)?,
        grid_prices: format_prices(&c.prices, steps.0)?,
        initial_base_quantity_estimate: c.initial_base,
        fee_reserve_usdt: c.reserve,
        minimum_order_usdt: s.min_order_usdt.max(market.rules.min_notional),
        worst_gross_grid_pct: c.gross,
        worst_net_grid_pct: c.net,
        minimum_net_grid_pct: s.min_net_pct,
        effective_cost_per_side_pct: s.fee_pct + s.slippage_pct,
        risk_budget_usdt: risk_budget(s),
        stop_scenario_loss_usdt: c.stop_loss,
        stress_scenario_loss_usdt: c.stress_loss,
        stress_exit_price: c.stress_exit,
        atr: market.atr,
        atr_period: s.atr_period,
        candle_interval: s.interval.clone(),
        data_source: market.data_source,
        atr_source: market.atr_source,
        closed_candle_count: market.closed_candle_count,
        last_candle_open_ms: market.last_candle_open_ms,
        market_as_of_ms: market.market_as_of_ms,
        rules: market.rules,
        history: market.history,
    })
}

/// 输入：设置和行情来源；返回：与实际计算假设有关的短提示。
fn warnings(s: &Settings, market: &Market) -> Vec<String> {
    let mut warnings = vec!["止损情景按单边下跌、订单完整成交及给定成本计算；跳空、流动性不足或未卖出可使亏损超过预算。".into(), // 与当前模型和数据来源对应的执行边界，随 JSON 一起导出。
        "TP 是停止机器人价格；上限以上通常已经卖完网格币，继续上涨不等于继续赚取网格利润。".into(),
        "请开启币安「停止时卖出全部基础币」，并核对创建预览里的实际数量、手续费预留和最低投入。".into()];
    if market.data_source == "offline_inputs" {
        warnings.push(
            "离线过滤器属于输入/示例值，未核实交易所现行规则；可用 --live 获取公开规则。".into(),
        );
    }
    if s.mode == RangeMode::Percent {
        warnings.push("百分比区间和 3% 默认缓冲是示例假设，没有用历史行情识别支撑阻力。".into());
    }
    if s.mode == RangeMode::Atr {
        warnings.push(
            "ATR 只衡量已发生的波动，不预测方向；倍数是可调参数，不是未来区间的概率保证。".into(),
        );
    }
    if let Some(history) = &market.history {
        // 把同批历史的短样本提醒带入方案，避免只在图表显示。
        warnings.extend(history.warnings.iter().cloned());
    }
    warnings
}
