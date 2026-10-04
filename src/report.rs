//! 文本展示：读取已验证的 Plan，生成中文填写参数与风险解释，不重新计算策略。

use crate::model::Plan;
use anyhow::Result;
use std::fmt::Write;

/// 输入：方案和是否展开价格；返回：面向初学者的可复制中文文本报告。
pub fn text_report(p: &Plan, levels: bool) -> Result<String> {
    let mut text = String::new(); // 逐段追加的中文报告缓冲，最终一次输出，避免部分成功内容。
    writeln!(text, "币安现货网格填写参数（{}）", p.symbol)?;
    writeln!(
        text,
        "模式：等比网格；区间模型：{}；参考价：{} USDT",
        p.range_model, p.reference_price
    )?;
    writeln!(
        text,
        "区间下限：{} USDT\n区间上限：{} USDT",
        p.lower_price, p.upper_price
    )?;
    writeln!(
        text,
        "网格数量：{}（{} 个价格点）",
        p.grid_count,
        p.grid_prices.len()
    )?;
    writeln!(
        text,
        "投入金额：{:.2} USDT（最多可投入 {:.2}；未分配 {:.2}）",
        p.investment_usdt, p.capital_limit_usdt, p.unallocated_usdt
    )?;
    writeln!(
        text,
        "止损 SL：{} USDT\n停止价 TP：{} USDT",
        p.stop_loss, p.take_profit
    )?;
    writeln!(text, "高级设置：停止时卖出全部基础币 = 开启")?;
    append_estimates(&mut text, p)?;
    if levels {
        writeln!(text, "\n网格价格（从下至上）：")?;
        for (i, price) in p.grid_prices.iter().enumerate() {
            // i 为价格点序号，price 为后端保留精度的字符串。
            writeln!(text, "  {i:>3}: {price}")?;
        }
    }
    writeln!(text, "\n计算边界：")?;
    for warning in &p.warnings {
        // 逐条保留计算假设，复制/保存报告时仍能看到风险边界。
        writeln!(text, "- {warning}")?;
    }
    Ok(text)
}

/// 输入：报告缓冲区与方案；返回：无；补充收益、风险和数据来源。
fn append_estimates(text: &mut String, p: &Plan) -> Result<()> {
    writeln!(text, "\n估算明细（币安实际下单数量以创建预览为准）：")?;
    writeln!(
        text,
        "每格统一基础币数量：{}；最低订单金额门槛 {:.2} USDT",
        p.quantity_per_grid, p.minimum_order_usdt
    )?;
    writeln!(
        text,
        "启动基础币估算：{:.8}；手续费预留成本：{:.2} USDT",
        p.initial_base_quantity_estimate, p.fee_reserve_usdt
    )?;
    writeln!(
        text,
        "最差一格毛收益 {:.4}%；双边扣费及成本后 {:.4}%（门槛 {:.4}%）",
        p.worst_gross_grid_pct, p.worst_net_grid_pct, p.minimum_net_grid_pct
    )?;
    writeln!(
        text,
        "单边费率与额外成本合计 {:.4}%（买卖各扣一次）",
        p.effective_cost_per_side_pct
    )?;
    writeln!(
        text,
        "账户风险预算：{:.2} USDT；单边下跌止损情景亏损：{:.2} USDT（账户 {:.2}%）",
        p.risk_budget_usdt,
        p.stop_scenario_loss_usdt,
        p.stop_scenario_loss_usdt / p.account_equity_usdt * 100.0
    )?;
    writeln!(
        text,
        "跌穿止损的压力情景：成交价 {:.4}，亏损 {:.2} USDT",
        p.stress_exit_price, p.stress_scenario_loss_usdt
    )?;
    append_sources(text, p)?;
    Ok(())
}

/// 输入：报告缓冲区与方案；返回：无；显示规则来源、ATR 周期和快照时间。
fn append_sources(text: &mut String, p: &Plan) -> Result<()> {
    writeln!(
        text,
        "数据来源：{}；价格步长 {}；数量步长 {}",
        p.data_source, p.rules.tick_size, p.rules.step_size
    )?;
    if let Some(atr) = p.atr {
        // 没有计算 ATR 的模式不展示虚构波幅。
        writeln!(
            text,
            "ATR({}) = {:.4}；K 线周期 {}；已收盘样本 {} 根；来源 {}",
            p.atr_period,
            atr,
            p.candle_interval,
            p.closed_candle_count,
            p.atr_source.as_deref().unwrap_or("unknown")
        )?;
    }
    if let Some(time) = p.market_as_of_ms {
        // time 为服务器 Unix 毫秒，离线报告不补造快照时间。
        writeln!(text, "行情读取时服务器时间：{time}（Unix 毫秒）")?;
    }
    Ok(())
}
