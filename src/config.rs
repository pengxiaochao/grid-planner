//! 配置层：字段单位、默认值、CLI 覆盖顺序与联网前校验集中在此处。

use anyhow::{Context, Result, ensure};
use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 区间模型枚举；模型只决定上下限的来源，不代表未来涨跌预测。
#[derive(Clone, Copy, Debug, Deserialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RangeMode {
    /// 以现价的上下百分比展开区间；参数属于用户假设。
    Percent,
    /// 以已收盘 K 线的 ATR 倍数展开区间；只衡量历史波动。
    Atr,
    /// 使用用户确认的上下限，程序只校验及计算资金约束。
    Manual,
}

/// 选参算法；经典模式保持旧调用，自适应模式必须有足量已收盘 OHLC。
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Algorithm {
    /// 只选择满足约束的最多格数，不进行收益回测。
    Classic,
    /// 三段滚动发展验证选参，加独立最终检验及观望门槛。
    Adaptive,
}

/// 合并 TOML、命令行和交互输入后的计算设置。
/// 所有 *_pct 字段使用“百分比数值”，金额以 USDT、数量以基础币计；未知 TOML 字段会被拒绝。
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// 本次允许投入的上限，单位 USDT；不是必须全部投入的金额。
    pub capital: f64,
    /// 离线参考现价，单位 USDT/基础币；联网时使用公开报价。
    pub price: f64,
    /// 账户总资产，单位 USDT；None 时以 capital 作为风险预算基数。
    pub equity: Option<f64>,
    /// 大写 USDT 现货交易对，例如 BTCUSDT；不支持合约或其他报价币。
    pub symbol: String,
    /// 区间来源：百分比、Wilder ATR 或手动上下限。
    pub mode: RangeMode,
    /// 格数/区间选参算法；默认 classic，保留既有 CLI/TOML 行为。
    pub algorithm: Algorithm,
    /// 手填的 ATR 绝对价格波幅，单位 USDT；None 表示从其他来源计算。
    pub atr: Option<f64>,
    /// 可选的用户 OHLC CSV 路径，仅离线 ATR 模式使用；不自动加载演示文件。
    pub candles: Option<PathBuf>,
    /// 固定 K 线周期字符串，例如 1d；同时决定连续性及收盘边界。
    pub interval: String,
    /// 可选历史请求根数，最多 1000；必须大于 atr_period 才能计算 ATR。
    pub history_bars: Option<usize>,
    /// Wilder ATR 平滑周期；计算首个真实波幅还需要一根前置 K 线。
    pub atr_period: usize,
    /// ATR 模式下，从现价向上下各展开多少倍 ATR。
    pub range_atr_mult: f64,
    /// ATR 模式下，从区间下限再向下缓冲多少倍 ATR。
    pub stop_atr_mult: f64,
    /// ATR 模式下，从区间上限再向上缓冲多少倍 ATR。
    pub take_atr_mult: f64,
    /// 手动模式的区间下限，单位 USDT；其他模式不接受此字段。
    pub lower: Option<f64>,
    /// 手动模式的区间上限，单位 USDT；现价必须位于上下限之间。
    pub upper: Option<f64>,
    /// 可选止损覆盖价，单位 USDT；None 自动推导，显式值必须低于下限。
    pub stop_loss: Option<f64>,
    /// 可选机器人停止价，单位 USDT；None 自动推导，显式值必须高于上限。
    pub take_profit: Option<f64>,
    /// 百分比模式向下幅度，填 10 表示 10%，不是 0.10。
    pub down_pct: f64,
    /// 百分比模式向上幅度，填 10 表示 10%。
    pub up_pct: f64,
    /// 非 ATR 模式的区间外止损/停止价缓冲，填 3 表示 3%。
    pub outside_pct: f64,
    /// 本策略止损情景占账户总资产的预算百分比；不保证实际亏损上限。
    pub risk_pct: f64,
    /// 买入、卖出各自的手续费百分比；两边分别计算，不重复扣双边费率。
    pub fee_pct: f64,
    /// 每边额外成本/滑点假设百分比，与 fee_pct 相加后用于情景计算。
    pub slippage_pct: f64,
    /// 用户要求的最低单笔金额，单位 USDT；与交易所最低金额取较大值。
    pub min_order_usdt: f64,
    /// 价格取整后最差一格扣除双边成本的收益门槛，单位百分比。
    pub min_net_pct: f64,
    /// 投入中预留手续费基础币成本的百分比；预留部分也计入下跌风险。
    pub reserve_pct: f64,
    /// 压力情景中，实际卖出价低于止损价的额外跌幅百分比。
    pub stress_pct: f64,
    /// 自动搜索的最多网格段数；还受交易所最大订单数限制。
    pub max_grids: usize,
    /// 可选固定网格段数；None 自动搜索，不可行的固定值会报错而非改值。
    pub grids: Option<usize>,
    /// 离线价格步长字符串，例如 0.01；保留十进制位数供精确输出。
    pub tick_size: String,
    /// 离线基础币数量步长字符串，例如 0.00001。
    pub step_size: String,
    /// 离线交易所单笔最低名义金额，单位 USDT；联网由实际过滤器覆盖。
    pub min_notional: f64,
    /// 离线交易所单笔最低基础币数量；不是 USDT 金额。
    pub min_qty: f64,
    /// 离线交易所单笔最高基础币数量；None 表示不添加此上限。
    pub max_qty: Option<f64>,
    /// 离线交易所价格下界，单位 USDT；None 表示此项未启用。
    pub min_price: Option<f64>,
    /// 离线交易所价格上界，单位 USDT；None 表示此项未启用。
    pub max_price: Option<f64>,
    /// 离线交易所单笔最高名义金额，单位 USDT；None 表示未启用。
    pub max_notional: Option<f64>,
    /// 可选公开行情代理 URL；不接受账号密码，显式代理连接失败不会直连回退。
    pub proxy_url: Option<String>,
}

impl Default for Settings {
    /// 输入：无；返回：可解释的示例默认值，资金和现价仍需用户提供。
    fn default() -> Self {
        Self {
            capital: 0.0,
            price: 0.0,
            equity: None,
            symbol: "BTCUSDT".into(),
            mode: RangeMode::Percent,
            algorithm: Algorithm::Classic,
            atr: None,
            candles: None,
            interval: "1d".into(),
            history_bars: None,
            atr_period: 14,
            range_atr_mult: 3.0,
            stop_atr_mult: 1.5,
            take_atr_mult: 1.5,
            lower: None,
            upper: None,
            stop_loss: None,
            take_profit: None,
            down_pct: 10.0,
            up_pct: 10.0,
            outside_pct: 3.0,
            risk_pct: 2.0,
            fee_pct: 0.1,
            slippage_pct: 0.05,
            min_order_usdt: 30.0,
            min_net_pct: 0.3,
            reserve_pct: 5.0,
            stress_pct: 5.0,
            max_grids: 150,
            grids: None,
            tick_size: "0.01".into(),
            step_size: "0.00001".into(),
            min_notional: 10.0,
            min_qty: 0.00001,
            max_qty: None,
            min_price: None,
            max_price: None,
            max_notional: None,
            proxy_url: None,
        }
    }
}

/// clap 命令行参数模型；Option 用于区分“未填写”与“显式覆盖”。
/// 参数帮助直接来自字段文档，布尔开关控制输入/输出流程，计算字段再合并到 Settings。
#[derive(Debug, Parser)]
#[command(
    version,
    about = "币安 USDT 现货网格参数生成器（只计算，不下单）",
    args_override_self = true
)]
pub struct Cli {
    /// TOML 配置路径；命令行显式参数优先
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// 交互输入本金、现价、账户资产和风险比例
    #[arg(long)]
    pub interactive: bool,
    /// 读取币安公开现价和过滤器；ATR 模式同时读取已收盘 K 线
    #[arg(long)]
    pub live: bool,
    /// 独立获取真实历史行情并输出 JSON；不需要填写本金或手动价格
    #[arg(long, conflicts_with_all = ["interactive", "candles", "atr"])]
    pub fetch_history: bool,
    /// 币安公开 API 地址；测试可用本地 HTTP 服务
    #[arg(long, default_value = "https://api.binance.com")]
    pub api_base_url: String,
    /// 手动代理，例如 http://127.0.0.1:7890 或 socks5h://127.0.0.1:1080
    #[arg(long)]
    pub proxy_url: Option<String>,
    /// 输出 JSON（价格和数量为精确格式的十进制字符串）
    #[arg(long)]
    pub json: bool,
    /// 在文本报告中展开所有网格价格
    #[arg(long)]
    pub levels: bool,
    /// 同时保存报告到文件
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// 本次最多可投入的 USDT
    #[arg(long)]
    pub capital: Option<f64>,
    /// 参考现价；联网时由公开报价覆盖
    #[arg(long)]
    pub price: Option<f64>,
    /// 账户总资产（USDT）；默认等于 capital
    #[arg(long)]
    pub equity: Option<f64>,
    /// 交易对，默认 BTCUSDT，仅支持 USDT 报价
    #[arg(long)]
    pub symbol: Option<String>,
    /// 区间模型，默认 percent：百分比 / ATR / 手动
    #[arg(long, value_enum)]
    pub mode: Option<RangeMode>,
    /// 选参算法 classic / adaptive；默认 classic；adaptive 使用已收盘历史滚动验证
    #[arg(long, value_enum)]
    pub algorithm: Option<Algorithm>,
    /// 手填 ATR 的价格数值，仅用于 atr 模式
    #[arg(long)]
    pub atr: Option<f64>,
    /// 已收盘 OHLC CSV，不能同时指定 atr 或 live
    #[arg(long)]
    pub candles: Option<PathBuf>,
    /// K 线周期：1m/5m/15m/30m/1h/4h/1d/1w，默认 1d
    #[arg(long)]
    pub interval: Option<String>,
    /// 请求最近多少根已收盘 K 线，须大于 ATR 周期且不超过 1000；历史获取默认 180
    #[arg(long)]
    pub history_bars: Option<usize>,
    /// ATR 周期，默认 14，范围 2..200
    #[arg(long)]
    pub atr_period: Option<usize>,
    /// 区间单侧 ATR 倍数，默认 3
    #[arg(long)]
    pub range_atr_mult: Option<f64>,
    /// 下限之外的止损 ATR 倍数，默认 1.5
    #[arg(long)]
    pub stop_atr_mult: Option<f64>,
    /// 上限之外的止盈 ATR 倍数，默认 1.5
    #[arg(long)]
    pub take_atr_mult: Option<f64>,
    /// 手动区间下限（mode=manual）
    #[arg(long)]
    pub lower: Option<f64>,
    /// 手动区间上限（mode=manual）
    #[arg(long)]
    pub upper: Option<f64>,
    /// 覆盖自动止损价，必须低于下限
    #[arg(long)]
    pub stop_loss: Option<f64>,
    /// 覆盖自动止盈价，必须高于上限
    #[arg(long)]
    pub take_profit: Option<f64>,
    /// 百分比区间下跌幅度，默认 10（代表 10%）
    #[arg(long)]
    pub down_pct: Option<f64>,
    /// 百分比区间上涨幅度，默认 10
    #[arg(long)]
    pub up_pct: Option<f64>,
    /// 非 ATR 模式的区间外缓冲百分比，默认 3
    #[arg(long)]
    pub outside_pct: Option<f64>,
    /// 单策略止损情景占账户资产的百分比，默认 2
    #[arg(long)]
    pub risk_pct: Option<f64>,
    /// 每一边的费率百分比，默认 0.1；0.075 代表 0.075%
    #[arg(long)]
    pub fee_pct: Option<f64>,
    /// 每一边的额外成本/滑点假设百分比，默认 0.05
    #[arg(long)]
    pub slippage_pct: Option<f64>,
    /// 最低一笔网格订单的建议金额，默认 30 USDT
    #[arg(long)]
    pub min_order_usdt: Option<f64>,
    /// 最差一格扣除双边成本后的收益门槛百分比，默认 0.3
    #[arg(long)]
    pub min_net_pct: Option<f64>,
    /// 手续费预留占投入的百分比，默认 5
    #[arg(long)]
    pub reserve_pct: Option<f64>,
    /// 跌穿止损后的压力情景跌幅百分比，默认 5
    #[arg(long)]
    pub stress_pct: Option<f64>,
    /// 格数搜索上限，默认 150，支持 2..170 的静态网格
    #[arg(long)]
    pub max_grids: Option<usize>,
    /// 强制指定格数；不可行时明确失败
    #[arg(long)]
    pub grids: Option<usize>,
    /// 离线价格最小步长，默认示例 0.01
    #[arg(long)]
    pub tick_size: Option<String>,
    /// 离线数量最小步长，默认示例 0.00001
    #[arg(long)]
    pub step_size: Option<String>,
    /// 离线交易所最低订单金额，默认示例 10 USDT
    #[arg(long)]
    pub min_notional: Option<f64>,
    /// 离线交易所最低数量，默认示例 0.00001
    #[arg(long)]
    pub min_qty: Option<f64>,
    /// 离线交易所最高单笔数量
    #[arg(long)]
    pub max_qty: Option<f64>,
    /// 离线价格下限
    #[arg(long)]
    pub min_price: Option<f64>,
    /// 离线价格上限
    #[arg(long)]
    pub max_price: Option<f64>,
    /// 离线单笔金额上限
    #[arg(long)]
    pub max_notional: Option<f64>,
}

impl Cli {
    /// 输入：CLI 与可选配置文件；返回：合并设置或读取/解析错误。
    pub fn settings(&self) -> Result<Settings> {
        // 先装载配置或默认值，稍后由命令行覆盖。
        let mut settings = if let Some(path) = &self.config {
            let content =
                std::fs::read_to_string(path) // 读取的 TOML 原文；文件读取失败直接返回错误。
                    .with_context(|| format!("无法读取配置 {}", path.display()))?;
            let mut parsed: Settings = // TOML 解析结果，拒绝拼错的未知字段。
                toml::from_str(&content).context("TOML 配置错误（请检查字段拼写）")?;
            if let Some(candles) = &parsed.candles {
                // 配置中的 CSV 路径；相对路径以 TOML 所在目录解释。
                if candles.is_relative() {
                    parsed.candles = Some(
                        path.parent()
                            .unwrap_or(std::path::Path::new("."))
                            .join(candles),
                    );
                }
            }
            parsed
        } else {
            Settings::default()
        };
        self.apply(&mut settings);
        Ok(settings)
    }

    /// 输入：待覆盖的设置；返回：无；只复制用户明确提供的 CLI 参数。
    fn apply(&self, settings: &mut Settings) {
        // 非 Option 目标字段只被显式 CLI 参数覆盖；value 是克隆后的用户输入。
        macro_rules! replace { ($($field:ident),*) => { $(if let Some(value) = &self.$field { settings.$field = value.clone(); })* }; }
        // Option 目标字段保留 None/Some 语义，未填写时不覆盖 TOML 的可选值。
        macro_rules! optional { ($($field:ident),*) => { $(if let Some(value) = &self.$field { settings.$field = Some(value.clone()); })* }; }
        replace!(
            capital,
            price,
            symbol,
            mode,
            algorithm,
            interval,
            atr_period,
            range_atr_mult,
            stop_atr_mult,
            take_atr_mult,
            down_pct,
            up_pct,
            outside_pct,
            risk_pct,
            fee_pct,
            slippage_pct,
            min_order_usdt,
            min_net_pct,
            reserve_pct,
            stress_pct,
            max_grids,
            tick_size,
            step_size,
            min_notional,
            min_qty
        );
        optional!(
            equity,
            atr,
            candles,
            lower,
            upper,
            stop_loss,
            take_profit,
            grids,
            max_qty,
            min_price,
            max_price,
            max_notional,
            proxy_url,
            history_bars
        );
    }
}

impl Settings {
    /// 输入：设置和联网标记；返回：有效设置的确认或错误。
    pub fn validate(&self, live: bool) -> Result<()> {
        positive("capital（本金）", self.capital)?;
        if !live {
            positive("price（参考现价）", self.price)?;
        }
        let equity = self.equity.unwrap_or(self.capital); // 风险预算使用的账户资产；未提供时取投入上限。
        positive("equity（账户资产）", equity)?;
        ensure!(
            equity >= self.capital,
            "账户资产 equity 不能小于可投入本金 capital"
        );
        self.validate_market()?;
        self.validate_rates()?;
        self.validate_mode(live)?;
        self.validate_algorithm(live)?;
        self.validate_limits()?;
        ensure!(
            live || self.history_bars.is_none(),
            "history-bars 需要 --live 或 --fetch-history"
        );
        Ok(())
    }

    /// 输入：算法与数据选择；返回：自适应证据及停止价约束的确认，经典模式不增加要求。
    fn validate_algorithm(&self, live: bool) -> Result<()> {
        if self.algorithm == Algorithm::Classic {
            return Ok(());
        }
        ensure!(self.mode == RangeMode::Atr, "自适应算法需要 mode=atr");
        ensure!(
            live || self.candles.is_some(),
            "自适应算法需要 --live 或已收盘 OHLC --candles，不能只用手填 ATR"
        );
        ensure!(
            self.stop_loss.is_none() && self.take_profit.is_none(),
            "自适应滚动验证不能使用固定绝对 SL/TP；请使用止损/停止价 ATR 倍数"
        );
        ensure!(
            2.0 * (self.fee_pct + self.slippage_pct) < 100.0,
            "自适应双倍每边成本必须小于 100%"
        );
        Ok(())
    }

    /// 输入：历史行情设置；返回：联网输入校验结果，不要求本金、现价或手动区间。
    pub fn validate_history(&self) -> Result<()> {
        self.validate_market()?;
        ensure!(
            self.atr.is_none() && self.candles.is_none(),
            "历史行情只读取公开 API，不能混用 atr 或 candles"
        );
        Ok(())
    }

    /// 输入：交易对、周期、样本数量和代理；返回：各联网入口共同的校验结果。
    fn validate_market(&self) -> Result<()> {
        ensure!(
            self.symbol.ends_with("USDT")
                && self.symbol.len() > 4
                && self
                    .symbol
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
            "仅支持大写的 USDT 现货交易对，例如 BTCUSDT"
        );
        self.validate_proxy()?;
        interval_ms(&self.interval)?;
        ensure!(
            (2..=200).contains(&self.atr_period),
            "atr-period 必须在 2..200 内"
        );
        if let Some(count) = self.history_bars {
            // 显式请求根数必须足以初始化 ATR，并遵守单次 API 上限。
            ensure!(
                count > self.atr_period && count <= 1000,
                "history-bars 必须大于 atr-period（{}）且不超过 1000",
                self.atr_period
            );
        }
        Ok(())
    }

    /// 输入：可选代理地址；返回：协议、主机与端口校验结果，不输出账号信息。
    fn validate_proxy(&self) -> Result<()> {
        let Some(proxy) = &self.proxy_url else {
            // 未指定代理不添加手动代理；指定后只验证这个地址。
            return Ok(());
        };
        let url = // 已解析的代理 URL，用于协议、认证信息及主机端口校验。
            reqwest::Url::parse(proxy).context("代理地址格式错误，需要协议、IP/主机和端口")?;
        ensure!(
            matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h"),
            "代理协议须为 http、https、socks5 或 socks5h"
        );
        ensure!(
            url.host_str().is_some() && url.port_or_known_default().is_some_and(|p| p > 0),
            "代理需要有效主机及 1..65535 的端口"
        );
        ensure!(
            url.username().is_empty() && url.password().is_none(),
            "当前代理设置只支持无需账号的代理"
        );
        ensure!(
            matches!(url.path(), "" | "/") && url.query().is_none() && url.fragment().is_none(),
            "代理地址不能包含路径、查询或片段"
        );
        Ok(())
    }

    /// 输入：格数、周期与可选交易边界；返回：有效限制的确认。
    fn validate_limits(&self) -> Result<()> {
        ensure!(
            (2..=170).contains(&self.max_grids),
            "max-grids 必须在 2..170 内，本程序使用静态网格模型"
        );
        if let Some(n) = self.grids {
            // 固定格数同样受用户搜索上限约束。
            ensure!(
                (2..=self.max_grids).contains(&n),
                "grids 必须在 2..max-grids 内"
            );
        }
        positive("min-order-usdt", self.min_order_usdt)?;
        positive("min-notional", self.min_notional)?;
        ensure!(
            self.min_qty.is_finite() && self.min_qty >= 0.0,
            "min-qty 必须为非负有限数"
        );
        for (name, value) in [
            // name 是错误提示字段名，value 是待校验的数值/可选限制。
            ("max-qty", self.max_qty),
            ("min-price", self.min_price),
            ("max-price", self.max_price),
            ("max-notional", self.max_notional),
            ("stop-loss", self.stop_loss),
            ("take-profit", self.take_profit),
        ] {
            if let Some(value) = value {
                // 只有实际启用的限制才做正数校验。
                positive(name, value)?;
            }
        }
        Ok(())
    }

    /// 输入：设置中的百分比；返回：有效性确认，所有百分比只在计算时除以 100。
    fn validate_rates(&self) -> Result<()> {
        for (name, value) in [
            // name 是错误提示字段名，value 是待校验的数值/可选限制。
            ("down-pct", self.down_pct),
            ("up-pct", self.up_pct),
            ("outside-pct", self.outside_pct),
            ("reserve-pct", self.reserve_pct),
        ] {
            ensure!(
                value.is_finite() && value > 0.0 && value < 100.0,
                "{name} 必须大于 0 且小于 100"
            );
        }
        ensure!(
            self.risk_pct.is_finite() && self.risk_pct > 0.0 && self.risk_pct <= 100.0,
            "risk-pct 必须大于 0 且不超过 100"
        );
        for (name, value) in [
            // name 是错误提示字段名，value 是待校验的数值/可选限制。
            ("fee-pct", self.fee_pct),
            ("slippage-pct", self.slippage_pct),
            ("stress-pct", self.stress_pct),
            ("min-net-pct", self.min_net_pct),
        ] {
            ensure!(
                value.is_finite() && (0.0..100.0).contains(&value),
                "{name} 必须为 0..100 内的有限数"
            );
        }
        ensure!(
            self.fee_pct + self.slippage_pct < 100.0,
            "单边总成本必须小于 100%"
        );
        for (name, value) in [
            // name 是错误提示字段名，value 是待校验的数值/可选限制。
            ("range-atr-mult", self.range_atr_mult),
            ("stop-atr-mult", self.stop_atr_mult),
            ("take-atr-mult", self.take_atr_mult),
        ] {
            positive(name, value)?;
        }
        Ok(())
    }

    /// 输入：区间模型和数据选择；返回：互斥关系和必需字段的确认。
    fn validate_mode(&self, live: bool) -> Result<()> {
        ensure!(
            !(self.atr.is_some() && self.candles.is_some()),
            "atr 和 candles 不能同时提供"
        );
        ensure!(
            !(live && (self.atr.is_some() || self.candles.is_some())),
            "live 使用公开 K 线，不能同时提供 atr 或 candles"
        );
        if self.mode == RangeMode::Manual {
            positive("lower", self.lower.context("manual 模式需要 lower")?)?;
            positive("upper", self.upper.context("manual 模式需要 upper")?)?;
        } else {
            ensure!(
                self.lower.is_none() && self.upper.is_none(),
                "lower/upper 只用于 manual 模式"
            );
        }
        if self.mode == RangeMode::Atr {
            ensure!(
                live || self.atr.is_some() || self.candles.is_some(),
                "atr 模式需要 --atr、--candles 或 --live"
            );
            if let Some(atr) = self.atr {
                // 手填波幅仍必须是有限正数，不接受 NaN 或零波动。
                positive("atr", atr)?;
            }
        } else {
            ensure!(
                self.atr.is_none() && self.candles.is_none(),
                "atr/candles 需要 mode=atr"
            );
        }
        Ok(())
    }
}

/// 输入：字段名和值；返回：正数确认或带字段名的错误，限制异常巨大输入。
pub fn positive(name: &str, value: f64) -> Result<()> {
    ensure!(
        value.is_finite() && value > 0.0 && value <= 1e12,
        "{name} 必须为大于 0、不超过 1e12 的有限数"
    );
    Ok(())
}

/// 输入：K 线周期；返回：固定周期的毫秒数，不支持长度不固定的月线。
pub fn interval_ms(interval: &str) -> Result<u64> {
    // 固定 K 线周期的分钟数，最后统一换算为毫秒。
    let minutes = match interval {
        "1m" => 1,
        "5m" => 5,
        "15m" => 15,
        "30m" => 30,
        "1h" => 60,
        "4h" => 240,
        "1d" => 1440,
        "1w" => 10080,
        _ => anyhow::bail!("interval 必须为 1m/5m/15m/30m/1h/4h/1d/1w"),
    };
    Ok(minutes * 60_000)
}
