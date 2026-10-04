//! 数据模型：在计算、JSON 导出和桌面端之间传递同一份带来源与时间的快照。

use crate::candles::Candle;
use crate::config::{Algorithm, GridMode, Settings};
use anyhow::{Context, Result, ensure};
use serde::Serialize;

/// 价格、基础币数量和订单金额的交易规则快照；不会持有账户或下单权限。
#[derive(Clone, Debug, Serialize)]
pub struct Rules {
    /// 价格步长字符串，例如 0.01；保留十进制位数供精确输出。
    pub tick_size: String,
    /// 基础币数量步长字符串，例如 0.00001。
    pub step_size: String,
    /// 单笔最低名义金额，单位 USDT；多个金额过滤器取更严格的最低值。
    pub min_notional: f64,
    /// 交易所单笔最低基础币数量；不是 USDT 金额。
    pub min_qty: f64,
    /// 交易所单笔最高基础币数量；None 表示不添加此上限。
    pub max_qty: Option<f64>,
    /// 交易所价格下界，单位 USDT；None 表示此项未启用。
    pub min_price: Option<f64>,
    /// 交易所价格上界，单位 USDT；None 表示此项未启用。
    pub max_price: Option<f64>,
    /// 交易所单笔最高名义金额，单位 USDT；None 表示未启用。
    pub max_notional: Option<f64>,
    /// 可选 MAX_NUM_ORDERS 订单数上限；None 时仍受程序的格数上限约束。
    pub max_orders: Option<usize>,
}

impl Rules {
    /// 输入：离线设置；返回：用户提供或示例过滤规则，不代表交易所实时规则。
    pub fn offline(settings: &Settings) -> Self {
        Self {
            tick_size: settings.tick_size.clone(),
            step_size: settings.step_size.clone(),
            min_notional: settings.min_notional,
            min_qty: settings.min_qty,
            max_qty: settings.max_qty,
            min_price: settings.min_price,
            max_price: settings.max_price,
            max_notional: settings.max_notional,
            max_orders: None,
        }
    }

    /// 输入：价格列表（可含止损止盈）；返回：交易所价格边界的确认。
    pub fn validate_prices(&self, prices: &[f64]) -> Result<()> {
        for price in prices {
            // 逐个核对区间、SL 和 TP 的交易所价格边界。
            ensure!(
                self.min_price.is_none_or(|min| *price >= min),
                "价格低于 PRICE_FILTER 的 minPrice"
            );
            ensure!(
                self.max_price.is_none_or(|max| *price <= max),
                "价格高于 PRICE_FILTER 的 maxPrice"
            );
        }
        Ok(())
    }
}

/// 一次加载得到的参考价、ATR、交易规则与来源元信息，供规划器统一消费。
pub struct Market {
    /// 本次计算的参考现价，单位 USDT；与其余字段组成一次读取快照。
    pub price: f64,
    /// 可选 Wilder ATR 绝对波幅，单位 USDT；百分比/手动模式可以不计算。
    pub atr: Option<f64>,
    /// 当前交易对的价格、数量、金额过滤器；来源由 data_source 标明。
    pub rules: Rules,
    /// 现价与规则来源标识：offline_inputs 或 binance_public_api。
    pub data_source: String,
    /// 可选 ATR 来源标识，区分手填值、用户 CSV 与已收盘公开 K 线。
    pub atr_source: Option<String>,
    /// 实际参与 ATR 计算的已收盘样本数；未读取历史时为 0。
    pub closed_candle_count: usize,
    /// 末根已收盘 K 线的 Unix 开盘毫秒；没有 K 线时为 None。
    pub last_candle_open_ms: Option<u64>,
    /// 本次读取的交易所服务器 Unix 毫秒；离线输入没有此时间。
    pub market_as_of_ms: Option<u64>,
    /// 可选同批公开 OHLC 快照，供桌面图表及导出复算；离线不伪造历史。
    pub history: Option<History>,
    /// 同批已校验 OHLC，供历史选参；离线 CSV 也保留，但不伪造公开 History。
    pub candles: Vec<Candle>,
}

/// 可独立导出的历史快照；保存真实样本、ATR 与获取时间，方便图表展示及复算。
#[derive(Serialize)]
pub struct History {
    /// 本快照所属的大写交易对。
    pub symbol: String,
    /// 实际请求的固定 K 线周期；时间边界按 UTC 解释。
    pub candle_interval: String,
    /// 本快照的 Wilder ATR 平滑周期。
    pub atr_period: usize,
    /// 用户希望获取的已收盘 K 线根数，不等同于实际返回根数。
    pub requested_candle_count: usize,
    /// 经过收盘、OHLC 与连续性校验后实际保留的根数。
    pub closed_candle_count: usize,
    /// 获取快照时的公开报价，单位 USDT；与历史末根收盘价可能不同。
    pub current_price: f64,
    /// 使用 candles 全部实际样本计算的 Wilder ATR，单位 USDT。
    pub atr: f64,
    /// 首根样本的 Unix 开盘毫秒，用于核对覆盖范围。
    pub first_candle_open_ms: u64,
    /// 末根样本的 Unix 收盘毫秒，等于开盘时间 + 周期 - 1。
    pub last_candle_close_ms: u64,
    /// 获取时的交易所服务器 Unix 毫秒，所有保留 K 线须在此前收盘。
    pub market_as_of_ms: u64,
    /// 实际使用的公开 K 线端点；便于识别正式 API 与验收模拟服务。
    pub source_url: String,
    /// 按开盘时间递增且连续的实际 OHLC 样本，包含复算 ATR 所需数据。
    pub candles: Vec<Candle>,
    /// 例如样本少于请求数量的说明；不补齐数据，不用本地示例替代。
    pub warnings: Vec<String>,
}

impl History {
    /// 输入：设置、已计算 ATR 的行情、API 地址、K 线及请求根数；返回：可复算的历史快照。
    pub fn new(
        s: &Settings,
        market: &Market,
        base: &str,
        candles: Vec<Candle>,
        requested: usize,
    ) -> Result<Self> {
        // 短历史的可见提示，保留真实根数并说明未补齐。
        let first = candles.first().context("历史 K 线为空")?.open_time; // 实际样本的首根开盘毫秒，空样本直接报错。
        let last = candles.last().context("历史 K 线为空")?.open_time; // 实际样本的末根开盘毫秒，稍后转换为收盘毫秒。
        let count = candles.len(); // 实际样本根数，不能直接使用请求根数填报。
        let warnings = if count < requested {
            vec![format!(
                "请求 {requested} 根，API 实际只有 {count} 根已收盘数据；ATR 使用实际样本，未补齐或替换。"
            )]
        } else {
            Vec::new()
        };
        Ok(Self {
            symbol: s.symbol.clone(),
            candle_interval: s.interval.clone(),
            atr_period: s.atr_period,
            requested_candle_count: requested,
            closed_candle_count: count,
            current_price: market.price,
            atr: market.atr.context("历史快照缺少 ATR")?,
            first_candle_open_ms: first,
            last_candle_close_ms: last
                .checked_add(crate::config::interval_ms(&s.interval)? - 1)
                .context("K 线时间超出范围")?,
            market_as_of_ms: market.market_as_of_ms.context("历史快照缺少服务器时间")?,
            source_url: format!("{}/api/v3/klines", base.trim_end_matches('/')),
            candles,
            warnings,
        })
    }
}

/// 最终可序列化的网格方案；同时保存填写参数、资金估算和风险假设。
/// 价格/数量输出字符串，百分比字段输出百分比数值；情景亏损不等于交易所保证的亏损上限。
#[derive(Serialize)]
pub struct Plan {
    /// 可填写币安的 USDT 现货交易对。
    pub symbol: String,
    /// 网格排列方式 geometric（等比）或 arithmetic（等差），序列化为小写字符串。
    pub mode: GridMode,
    /// 区间计算模型标识：percent、atr 或 manual。
    pub range_model: String,
    /// classic 或 adaptive；算法与区间模型分别标记，避免把 ATR 当成收益优化。
    pub algorithm: Algorithm,
    /// 自适应发展段、旧策略对照及独立最终检验；经典模式为 None。
    pub optimization: Option<crate::optimizer::OptimizationReport>,
    /// 用户允许投入的 USDT 上限，保留供界面核对。
    pub capital_limit_usdt: f64,
    /// 计算风险预算所用的账户总资产，单位 USDT。
    pub account_equity_usdt: f64,
    /// 满足全部约束后的建议投入，单位 USDT；向上取整到分。
    pub investment_usdt: f64,
    /// capital_limit_usdt 中未投入的金额，单位 USDT。
    pub unallocated_usdt: f64,
    /// 用于启动库存与资金估算的现价，单位 USDT/基础币。
    pub reference_price: f64,
    /// 向下按 tickSize 取整的区间下限，十进制字符串避免显示浮点尾差。
    pub lower_price: String,
    /// 向上按 tickSize 取整的区间上限，单位 USDT。
    pub upper_price: String,
    /// 区间之外的止损触发价字符串；不是成交价保证。
    pub stop_loss: String,
    /// 区间之外的机器人停止价字符串；上限之上通常已卖完网格库存。
    pub take_profit: String,
    /// 网格段数 N；对应 N+1 个价格点，不能把两者混为一谈。
    pub grid_count: usize,
    /// 格数来自固定输入、最多可行搜索或发展段选优；解释资金增大为何不必增加格数。
    pub grid_count_reason: String,
    /// 按 stepSize 向下取整的每格统一基础币数量字符串。
    pub quantity_per_grid: String,
    /// 从下限到上限的 N+1 个严格递增价格字符串。
    pub grid_prices: Vec<String>,
    /// 启动时需购买的基础币估算，含高于现价的卖格及费用预留币。
    pub initial_base_quantity_estimate: f64,
    /// 预留基础币在现价下的购买成本，单位 USDT；包含在总投入中。
    pub fee_reserve_usdt: f64,
    /// 用户单格金额门槛与交易所 minNotional 的较大值，单位 USDT。
    pub minimum_order_usdt: f64,
    /// 取整后所有相邻格中最低的毛收益百分比。
    pub worst_gross_grid_pct: f64,
    /// 取整后所有相邻格中最低的双边扣费/成本净收益百分比。
    pub worst_net_grid_pct: f64,
    /// 用户设定的最差一格净收益门槛百分比。
    pub minimum_net_grid_pct: f64,
    /// 每边手续费加额外成本的百分比；买卖各计算一次。
    pub effective_cost_per_side_pct: f64,
    /// 账户资产 × risk_pct / 100，单位 USDT；只约束模型止损情景。
    pub risk_budget_usdt: f64,
    /// 单边下跌、完整成交且按 SL 卖出的情景亏损，单位 USDT。
    pub stop_scenario_loss_usdt: f64,
    /// 跌穿 SL 后再下跌 stress_pct 时的情景亏损，可能超过预算。
    pub stress_scenario_loss_usdt: f64,
    /// 压力情景假设的实际卖出价格，单位 USDT。
    pub stress_exit_price: f64,
    /// 可选 Wilder ATR 绝对波幅，单位 USDT；百分比/手动模式可以不计算。
    pub atr: Option<f64>,
    /// 本次使用/标注的 ATR 周期；未计算 ATR 时仅为配置值。
    pub atr_period: usize,
    /// 用于 ATR 或历史请求的 K 线周期。
    pub candle_interval: String,
    /// 现价与规则来源标识：offline_inputs 或 binance_public_api。
    pub data_source: String,
    /// 可选 ATR 来源标识，区分手填值、用户 CSV 与已收盘公开 K 线。
    pub atr_source: Option<String>,
    /// 实际参与 ATR 计算的已收盘样本数；未读取历史时为 0。
    pub closed_candle_count: usize,
    /// 末根已收盘 K 线的 Unix 开盘毫秒；没有 K 线时为 None。
    pub last_candle_open_ms: Option<u64>,
    /// 本次读取的交易所服务器 Unix 毫秒；离线输入没有此时间。
    pub market_as_of_ms: Option<u64>,
    /// 计算使用的交易规则快照，供创建机器人前核对。
    pub rules: Rules,
    /// 计算假设、数据来源和实际执行边界的中文提醒。
    pub warnings: Vec<String>,
    /// 可选同批公开 OHLC 快照，供桌面图表及导出复算；离线不伪造历史。
    pub history: Option<History>,
}
