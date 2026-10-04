//! 终端交互：仅补充必要资金参数；提示走 stderr，结果走 stdout。

use crate::config::Settings;
use anyhow::{Context, Result, ensure};
use std::io::{self, Write};

/// 输入：现有设置、联网标记；返回：无；在错误输出上提示，不污染 JSON 标准输出。
pub fn interactive(settings: &mut Settings, live: bool) -> Result<()> {
    eprintln!("填入 USDT 现货网格信息。百分比直接填 2 表示 2%；回车保留已有值。");
    settings.capital = ask("最多可投入本金（USDT）", settings.capital)?;
    if !live {
        settings.price = ask("币安当前现价（不是示例价格）", settings.price)?;
    }
    settings.equity = Some(ask(
        "账户总资产（USDT）",
        settings.equity.unwrap_or(settings.capital),
    )?);
    settings.risk_pct = ask("单策略止损情景占账户资产百分比", settings.risk_pct)?;
    Ok(())
}

/// 输入：提示和默认值；返回：用户数值，EOF 或非法文本立即报错。
fn ask(label: &str, default: f64) -> Result<f64> {
    eprint!("{label} [{default}]：");
    io::stderr().flush()?;
    let mut input = String::new(); // 用户输入缓冲；空行沿用默认值，EOF 和非法数字返回错误。
    ensure!(
        io::stdin().read_line(&mut input)? > 0,
        "输入已结束；请补全信息或使用命令行参数"
    );
    if input.trim().is_empty() {
        return Ok(default);
    }
    input
        .trim()
        .parse()
        .with_context(|| format!("{label} 需要数字"))
}
