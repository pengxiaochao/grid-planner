//! 命令行入口：合并配置、选择历史获取或网格生成，并保持 JSON 标准输出纯净。

mod candles;
mod config;
mod data;
mod input;
mod model;
mod planner;
mod precision;
mod report;

use anyhow::{Context, Result};
use clap::Parser;
use std::io::Write;

/// 输入：进程命令行及标准输入；返回：成功码 0，失败码 1 或 clap 参数错误码。
fn main() {
    if let Err(error) = run() {
        // 完整错误链只写 stderr，标准输出不会留下伪成功 JSON。
        eprintln!("错误：{error:#}");
        std::process::exit(1);
    }
}

/// 输入：CLI 和可选交互信息；返回：报告已输出的确认或完整错误链。
fn run() -> Result<()> {
    // 待保存并输出的完整报告或 JSON 文本。
    let cli = config::Cli::parse(); // 原始 CLI 选择，包含流程开关及显式参数。
    let mut settings = cli.settings()?; // 已合并配置的设置，必要时由交互输入补齐。
    let output = if cli.fetch_history {
        settings.history_bars.get_or_insert(180);
        settings.validate_history()?;
        let market = data::load(&settings, true, &cli.api_base_url)?; // 本次数据加载的行情及规则快照。
        let history = market.history.context("公开行情未返回历史数据")?; // 只来自本次公开请求的历史快照，不回退到旧文件。
        format!("{}\n", serde_json::to_string_pretty(&history)?)
    } else {
        if cli.interactive || std::env::args_os().len() == 1 {
            input::interactive(&mut settings, cli.live)?;
        }
        settings.validate(cli.live)?;
        let market = data::load(&settings, cli.live, &cli.api_base_url)?; // 本次数据加载的行情及规则快照。
        let plan = planner::generate(&settings, market)?; // 经过全部约束检查的网格方案。
        if cli.json {
            format!("{}\n", serde_json::to_string_pretty(&plan)?)
        } else {
            report::text_report(&plan, cli.levels)?
        }
    };
    write_output(&output, cli.output.as_deref())
}

/// 输入：报告或历史 JSON、可选保存路径；返回：文件和标准输出都成功写入的确认。
fn write_output(output: &str, path: Option<&std::path::Path>) -> Result<()> {
    if let Some(path) = path {
        // 只有用户指定输出路径时写文件；失败也不继续输出成功报告。
        std::fs::write(path, output).with_context(|| format!("无法保存报告 {}", path.display()))?;
    }
    std::io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .context("无法输出报告")?;
    Ok(())
}
