//! K 线与 ATR：校验真实 OHLC/时间连续性，再按 Wilder 方法计算价格波幅。

use crate::config::{interval_ms, positive};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::path::Path;

/// 一根 OHLC K 线；开盘时间按 Unix 毫秒保存，价格按报价币 USDT 计。
/// 结构自身不假定已经收盘，调用方还须依据服务器时间筛选。
#[derive(Debug, Serialize)]
pub struct Candle {
    /// Unix 开盘毫秒；用于排序、查重及检测缺失周期。
    pub open_time: u64,
    /// 周期内开盘价，单位 USDT。
    pub open: f64,
    /// 周期内最高价，须不小于开盘价和收盘价。
    pub high: f64,
    /// 周期内最低价，须不大于开盘价和收盘价。
    pub low: f64,
    /// 周期内收盘价；前一根 close 参与当前 TR 的跳空计算。
    pub close: f64,
}

impl Candle {
    /// 输入：单根 K 线；返回：OHLC 关系与有限价格的确认。
    pub fn validate(&self) -> Result<()> {
        for (name, price) in [
            // 逐个验证 OHLC 价格，name 用于说明哪个字段非法。
            ("open", self.open),
            ("high", self.high),
            ("low", self.low),
            ("close", self.close),
        ] {
            positive(name, price)?;
        }
        ensure!(
            self.low <= self.open
                && self.open <= self.high
                && self.low <= self.close
                && self.close <= self.high,
            "K 线要求 low <= open/close <= high"
        );
        Ok(())
    }
}

/// 输入：CSV 路径；返回：按文件顺序读取的 K 线，格式为五列未加引号数字。
pub fn read_csv(path: &Path) -> Result<Vec<Candle>> {
    let content = std::fs::read_to_string(path) // 用户提供的 CSV 原文，仅支持明确的五列数字格式。
        .with_context(|| format!("无法读取 K 线 {}", path.display()))?;
    let mut lines = content.lines(); // 逐行读取器，第一行必须是固定 OHLC 表头。
    ensure!(
        lines
            .next()
            .map(|line| line.trim().trim_start_matches('\u{feff}'))
            == Some("open_time,open,high,low,close"),
        "CSV 表头必须为 open_time,open,high,low,close"
    );
    lines
        .enumerate()
        .filter(|(_, row)| !row.trim().is_empty())
        .map(|(i, row)| parse_csv_row(row).with_context(|| format!("CSV 第 {} 行错误", i + 2)))
        .collect()
}

/// 输入：一行未加引号的 CSV；返回：单根 K 线或字段错误。
fn parse_csv_row(row: &str) -> Result<Candle> {
    let values: Vec<_> = row.split(',').map(str::trim).collect(); // 按开盘时间、开、高、低、收顺序拆出的五个字段。
    ensure!(values.len() == 5, "每根 K 线需要五列数据");
    // 待校验的单根 OHLC，非法价格关系不能进入 ATR。
    let candle = Candle {
        open_time: values[0]
            .parse()
            .context("open_time 必须为 Unix 毫秒整数")?,
        open: values[1].parse()?,
        high: values[2].parse()?,
        low: values[3].parse()?,
        close: values[4].parse()?,
    };
    candle.validate()?;
    Ok(candle)
}

/// 输入：按时间排序的已收盘 K 线和周期；返回：连续性确认或明确错误。
pub fn validate_series(candles: &[Candle], interval: &str) -> Result<()> {
    let step = interval_ms(interval)?; // 每根 K 线的固定毫秒跨度，用于发现漏根或重复。
    for candle in candles {
        // 每根都先校验价格关系，不能只检查最后一根。
        candle.validate()?;
    }
    for pair in candles.windows(2) {
        // pair 是相邻两根，时间差必须恰好等于一个周期。
        ensure!(
            pair[0].open_time.checked_add(step) == Some(pair[1].open_time),
            "K 线时间必须递增、不能重复或缺少周期；请检查 interval={interval}"
        );
    }
    Ok(())
}

/// 输入：至少 period+1 根连续已收盘 K 线、ATR 周期；返回：Wilder 平滑 ATR。
pub fn wilder_atr(candles: &[Candle], period: usize) -> Result<f64> {
    ensure!(
        candles.len() > period,
        "ATR({period}) 至少需要 {} 根已收盘 K 线，实际 {} 根",
        period + 1,
        candles.len()
    );
    let ranges: Vec<f64> = candles // 相邻 K 线的真实波幅 TR，包含前收盘价造成的跳空波动。
        .windows(2)
        .map(|pair| {
            let (previous, current) = (&pair[0], &pair[1]); // 前根收盘与当前高低价共同决定 TR。
            (current.high - current.low)
                .max((current.high - previous.close).abs())
                .max((current.low - previous.close).abs())
        })
        .collect();
    // 逐根按 (旧 ATR × (period-1) + 当前 TR) / period 更新的波幅。
    let initial = ranges[..period].iter().sum::<f64>() / period as f64; // 前 period 个 TR 的算术平均，作为 Wilder 平滑的初值。
    let atr = ranges[period..].iter().fold(initial, |atr, tr| {
        (atr * (period - 1) as f64 + tr) / period as f64
    });
    positive("计算所得 ATR（零波动不能生成 ATR 网格）", atr)?;
    Ok(atr)
}
