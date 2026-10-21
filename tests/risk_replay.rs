//! 压力风控和细周期/滚动验证的真实 CLI E2E；合成行情仅用于验收行为。
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

/// 输入：真实 CLI 参数；返回：退出状态及标准输出/错误，不模拟规划器。
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_grid-planner"))
        .args(args)
        .output()
        .unwrap()
}

/// 输入：参数；返回：成功 JSON，失败保留完整错误说明。
fn plan(args: &[&str]) -> Value {
    let o = run(args);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

/// 输入：日序号和是否修改尾部；返回：连续合成日线 OHLC。
fn day(i: usize, tail: bool) -> [f64; 4] {
    let close = |n: f64| 84000.0 + 3000.0 * (n * std::f64::consts::TAU / 12.0).sin();
    let open = close(i as f64 - 1.0);
    let end = if tail && i >= 160 {
        84000.0 + i as f64
    } else {
        close(i as f64)
    };
    [open, open.max(end) + 1200.0, open.min(end) - 1200.0, end]
}

/// 输入：日线与日内小时位置；返回：先低后高的确定性价格，小时聚合严格等于日线。
fn hourly_price(c: [f64; 4], hour: usize) -> f64 {
    let nodes = [c[0], c[2], c[1], c[3]];
    let segment = (hour / 8).min(2);
    let part = (hour - segment * 8) as f64 / 8.0;
    nodes[segment] + (nodes[segment + 1] - nodes[segment]) * part
}

/// 输入：目录和尾部场景；返回：180 根日线及精确覆盖同区间的 4320 根小时 CSV。
fn history(dir: &TempDir, tail: bool) -> (PathBuf, PathBuf) {
    let daily = dir.path().join(format!("daily-{tail}.csv"));
    let fine = dir.path().join(format!("hourly-{tail}.csv"));
    let mut a = "open_time,open,high,low,close\n".to_string();
    let mut b = a.clone();
    for i in 0..180 {
        let c = day(i, tail);
        let time = 1_700_000_000_000_u64 + i as u64 * 86_400_000;
        a.push_str(&format!("{time},{},{},{},{}\n", c[0], c[1], c[2], c[3]));
        for hour in 0..24 {
            let open = hourly_price(c, hour);
            let end = hourly_price(c, hour + 1);
            b.push_str(&format!(
                "{},{open},{},{},{end}\n",
                time + hour as u64 * 3_600_000,
                open.max(end),
                open.min(end)
            ));
        }
    }
    std::fs::write(&daily, a).unwrap();
    std::fs::write(&fine, b).unwrap();
    (daily, fine)
}

/// 输入：原周期 CSV 和额外选项；返回：真实自适应结果，资金充足以隔离回放逻辑。
fn adaptive(path: &Path, extra: &[&str]) -> Value {
    let mut args = vec![
        "--json",
        "--capital",
        "3000",
        "--equity",
        "15000",
        "--price",
        "84000",
        "--mode",
        "atr",
        "--algorithm",
        "adaptive",
        "--grids",
        "4",
        "--candles",
        path.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    plan(&args)
}

/// 输入：网格类型和压力跌幅；返回：固定截图线位的真实 CLI 方案，隔离仓位预算计算。
fn pressure_plan(mode: &str, stress: &str) -> Value {
    plan(&[
        "--json",
        "--capital",
        "663",
        "--equity",
        "663",
        "--risk-pct",
        "15",
        "--price",
        "82682.01",
        "--mode",
        "atr",
        "--atr",
        "2117.78",
        "--range-atr-mult",
        "4.5",
        "--grids",
        "16",
        "--grid-mode",
        mode,
        "--stress-pct",
        stress,
        "--min-order-usdt",
        "5",
    ])
}

/// 输入：无；返回：无；正常与压力损失均不得突破预算，费用与启动现金独立复算守恒。
#[test]
fn pressure_budget_and_initial_funding_are_hard_constraints() {
    for mode in ["geometric", "arithmetic"] {
        for stress in ["0", "5", "30"] {
            let p = pressure_plan(mode, stress);
            let budget = p["risk_budget_usdt"].as_f64().unwrap();
            for key in ["stop_scenario_loss_usdt", "stress_scenario_loss_usdt"] {
                assert!(p[key].as_f64().unwrap() <= budget + 1e-8, "{key}");
            }
            let investment = p["investment_usdt"].as_f64().unwrap();
            let funding = &p["funding"];
            let total: f64 = [
                "initial_grid_purchase_usdt",
                "buy_order_cash_usdt",
                "fee_reserve_usdt",
            ]
            .iter()
            .map(|k| funding[k].as_f64().unwrap())
            .sum();
            assert!((investment - total).abs() < 1e-8);
            assert!(
                (funding["fee_reserve_usdt"].as_f64().unwrap()
                    - p["fee_reserve_usdt"].as_f64().unwrap())
                .abs()
                    < 1e-8
            );
            let qty: f64 = p["quantity_per_grid"].as_str().unwrap().parse().unwrap();
            let cost = p["effective_cost_per_side_pct"].as_f64().unwrap() / 100.0;
            let base =
                16.0 * qty + p["fee_reserve_usdt"].as_f64().unwrap() / (82682.01 * (1.0 + cost));
            let loss = investment - base * p["stress_exit_price"].as_f64().unwrap() * (1.0 - cost);
            assert!((loss - p["stress_scenario_loss_usdt"].as_f64().unwrap()).abs() < 1e-8);
        }
    }
}

/// 输入：无；返回：无；细周期实际替代成交路径，保留日线 ATR 并报告独立滚动窗口和超额收益。
#[test]
fn hourly_execution_preserves_daily_atr_and_reports_oos_excess() {
    let dir = TempDir::new().unwrap();
    let (daily, fine) = history(&dir, false);
    let coarse = adaptive(&daily, &[]);
    let p = adaptive(
        &daily,
        &[
            "--replay-interval",
            "1h",
            "--replay-candles",
            fine.to_str().unwrap(),
        ],
    );
    assert_eq!(coarse["atr"], p["atr"]);
    let o = &p["optimization"];
    assert_eq!(o["execution_interval"], "1h");
    assert_eq!(o["holdout"]["replayed_bars"], 36 * 24);
    assert_eq!(o["rolling_validation"].as_array().unwrap().len(), 3);
    let h = &o["holdout"];
    let excess = h["candidate"]["net_return_pct"].as_f64().unwrap()
        - h["buy_hold_return_pct"].as_f64().unwrap();
    assert!((excess - h["excess_return_pct_points"].as_f64().unwrap()).abs() < 1e-8);
    for fold in o["rolling_validation"].as_array().unwrap() {
        assert!(fold["error"].is_null());
        let f = &fold["evaluation"];
        assert!(
            f["train_end_open_ms"].as_u64().unwrap()
                < f["evaluation_start_open_ms"].as_u64().unwrap()
        );
        assert_eq!(f["replayed_bars"], 20 * 24);
    }
    assert_ne!(
        coarse["optimization"]["holdout"]["candidate"],
        h["candidate"]
    );
}

/// 输入：无；返回：无；细周期缺根或聚合不一致必须失败，不允许静默回退到日线。
#[test]
fn incomplete_or_inconsistent_execution_history_is_rejected() {
    let dir = TempDir::new().unwrap();
    let (daily, fine) = history(&dir, false);
    let original = std::fs::read_to_string(&fine).unwrap();
    for bad in [
        original.lines().take(1000).collect::<Vec<_>>().join("\n"),
        original.replacen("85200", "95200", 1),
    ] {
        std::fs::write(&fine, bad).unwrap();
        let o = run(&[
            "--json",
            "--capital",
            "3000",
            "--price",
            "84000",
            "--mode",
            "atr",
            "--algorithm",
            "adaptive",
            "--candles",
            daily.to_str().unwrap(),
            "--replay-interval",
            "1h",
            "--replay-candles",
            fine.to_str().unwrap(),
        ]);
        assert!(!o.status.success());
        assert!(o.stdout.is_empty());
        assert!(String::from_utf8_lossy(&o.stderr).contains("回放"));
    }
}

/// 输入：无；返回：无；更改后续行情不得改变较早滚动窗口的选参、指标或时间范围。
#[test]
fn rolling_selection_cannot_see_later_windows() {
    let dir = TempDir::new().unwrap();
    let (a, _) = history(&dir, false);
    let (b, _) = history(&dir, true);
    let a = adaptive(&a, &[]);
    let b = adaptive(&b, &[]);
    let first = &a["optimization"]["rolling_validation"][0];
    assert!(first.is_object());
    assert_eq!(first, &b["optimization"]["rolling_validation"][0]);
    assert_ne!(
        a["optimization"]["rolling_validation"][2],
        b["optimization"]["rolling_validation"][2]
    );
}
