//! CLI 端到端验收：启动真实二进制、核对导出及独立资金公式，并通过本地 HTTP/SOCKS 服务验证联网链路。
//! 所有合成 K 线仅作为测试输入；生产应用不会自动读取这些数据。

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output, Stdio};
use std::thread::{self, JoinHandle};
use tempfile::TempDir;

/// 输入：命令行参数；返回：真实程序的退出码、标准输出和错误输出。
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_grid-planner"))
        .args(args)
        .output()
        .expect("应能启动已编译的 CLI")
}

/// 输入：额外 CLI 参数；返回：默认示例成功生成的 JSON。
fn plan(extra: &[&str]) -> Value {
    let mut args = vec!["--capital", "600", "--price", "84000", "--json"]; // 本用例传给真实 CLI 的参数数组，显式保留输入/输出选择。
    args.extend_from_slice(extra);
    let output = run(&args); // 真实 CLI 的退出状态、标准输出及错误信息。
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("标准输出应为 JSON")
}

/// 输入：JSON 中的十进制价格或数量字符串；返回：用于独立断言的浮点数。
fn number(value: &Value) -> f64 {
    value
        .as_str()
        .expect("应以十进制字符串输出")
        .parse()
        .unwrap()
}

/// 输入：JSON 方案；返回：无；检查从资金和逐格订单重新推导的约束。
fn check_invariants(p: &Value) {
    let n = p["grid_count"].as_u64().unwrap() as usize; // JSON 中的网格段数，重建资金时使用 N 格而非 N+1 点。
    let levels = p["grid_prices"].as_array().unwrap(); // 从导出 JSON 读取的全部价格点，用于独立核验顺序和成本。
    let qty = number(&p["quantity_per_grid"]); // 导出的每格基础币数量，独立计算库存与单笔金额。
    let low = number(&p["lower_price"]); // 导出的下限价格，最低单笔金额应在该价也达标。
    let price = p["reference_price"].as_f64().unwrap(); // 导出的启动参考现价，高价卖格需要按该价先购币。
    let fee = p["effective_cost_per_side_pct"].as_f64().unwrap() / 100.0; // 导出的单边总成本比例，买入/卖出各计一次。
    let invest = p["investment_usdt"].as_f64().unwrap(); // 建议投入金额，需同时满足本金上限与风险预算。
    assert_eq!(levels.len(), n + 1);
    assert!(number(&p["stop_loss"]) < low && low < price);
    assert!(price < number(&p["upper_price"]));
    assert!(number(&p["upper_price"]) < number(&p["take_profit"]));
    assert!(invest <= p["capital_limit_usdt"].as_f64().unwrap());
    assert!(
        p["stop_scenario_loss_usdt"].as_f64().unwrap() <= p["risk_budget_usdt"].as_f64().unwrap()
    );
    assert!(qty * low >= p["minimum_order_usdt"].as_f64().unwrap() - 1e-8);
    let funding: f64 = levels[..n] // 按 JSON 价位独立重建的全部网格购买成本，不调用规划器内部函数。
        .iter()
        .map(|v| number(v).min(price) * qty * (1.0 + fee))
        .sum();
    assert!(funding < invest);
    let reserve = invest - funding; // 投入减网格成本的预留部分，后续复算其跌价风险。
    let sl = number(&p["stop_loss"]); // JSON 中的 SL 价格，供独立清仓情景核算。
    let loss = funding - n as f64 * qty * sl * (1.0 - fee) // 独立重建的全库存及预留币止损亏损，应与报告值一致。
        + reserve * (1.0 - sl * (1.0 - fee) / (price * (1.0 + fee)));
    assert!((loss - p["stop_scenario_loss_usdt"].as_f64().unwrap()).abs() < 1e-7);
    for pair in levels.windows(2) {
        let (buy, sell) = (number(&pair[0]), number(&pair[1])); // 相邻点分别作为该格买入/卖出价。
        assert!(sell > buy);
        let net = ((1.0 - fee) * sell / buy - 1.0 - fee) * 100.0; // 按相邻买卖价独立复算的双边扣费净收益百分比。
        assert!(net + 1e-8 >= p["minimum_net_grid_pct"].as_f64().unwrap());
    }
}

/// 输入：无；返回：无；验收默认示例和可复制的币安参数。
#[test]
fn default_plan_obeys_budget_and_grid_math() {
    let p = plan(&[]); // 真实 CLI 标准输出解码的方案，所有断言读取此结果。
    assert_eq!(p["lower_price"], "75600.00");
    assert_eq!(p["upper_price"], "92400.00");
    assert_eq!(p["stop_loss"], "73332.00");
    assert_eq!(p["take_profit"], "95172.00");
    assert_eq!(p["risk_budget_usdt"], 12.0);
    assert!(p["investment_usdt"].as_f64().unwrap() < 600.0);
    assert!(
        p["stress_scenario_loss_usdt"].as_f64().unwrap()
            > p["stop_scenario_loss_usdt"].as_f64().unwrap()
    );
    check_invariants(&p);
}

/// 输入：无；返回：无；用公开公式独立验证单边手续费没有重复扣除。
#[test]
fn fee_formula_matches_binance_geometric_example() {
    // 真实 CLI 标准输出解码的方案，所有断言读取此结果。
    let p = plan(&[
        "--mode",
        "manual",
        "--lower",
        "400",
        "--upper",
        "450",
        "--price",
        "425",
        "--equity",
        "30000",
        "--grids",
        "5",
        "--fee-pct",
        "0.1",
        "--slippage-pct",
        "0",
        "--tick-size",
        "0.00000001",
        "--step-size",
        "0.001",
        "--min-net-pct",
        "0",
    ]);
    let expected = ((1.0 - 0.001) * (450_f64 / 400.0).powf(0.2) - 1.0 - 0.001) * 100.0; // 参考示例的独立数学结果，不复用被测规划器算法。
    assert!((p["worst_net_grid_pct"].as_f64().unwrap() - expected).abs() < 1e-7);
    check_invariants(&p);
}

/// 输入：无；返回：无；粗精度下仍遵守预算和整数步长。
#[test]
fn rounded_prices_and_quantity_stay_feasible() {
    // 真实 CLI 标准输出解码的方案，所有断言读取此结果。
    let p = plan(&[
        "--equity",
        "3000",
        "--tick-size",
        "10",
        "--step-size",
        "0.0001",
    ]);
    check_invariants(&p);
    for price in p["grid_prices"].as_array().unwrap() {
        assert!((number(price) / 10.0).fract().abs() < 1e-9);
    }
    assert!(
        ((number(&p["quantity_per_grid"]) / 0.0001).round() * 0.0001
            - number(&p["quantity_per_grid"]))
        .abs()
            < 1e-9
    );
}

/// 输入：无；返回：无；所有非法输入必须失败退出。
#[test]
fn invalid_inputs_never_produce_a_plan() {
    // 预先列出的非法输入组合；每种都应报错且不输出成功方案。
    let cases: &[&[&str]] = &[
        &["--capital", "0"],
        &["--price", "0"],
        &["--capital", "NaN"],
        &["--price", "inf"],
        &["--capital=-1"],
        &["--down-pct", "100"],
        &["--up-pct", "0"],
        &["--risk-pct", "0"],
        &["--risk-pct", "101"],
        &["--equity", "100"],
        &["--mode", "manual", "--lower", "90000", "--upper", "80000"],
        &["--mode", "manual", "--lower", "90000", "--upper", "95000"],
        &["--stop-loss", "80000"],
        &["--take-profit", "90000"],
        &["--grids", "1"],
        &["--max-grids", "171"],
        &["--tick-size", "0"],
        &["--step-size", "NaN"],
        &["--symbol", "ETHBTC"],
        &["--mode", "atr"],
        &["--mode", "atr", "--atr", "0"],
        &["--mode", "atr", "--atr", "2500", "--candles", "missing.csv"],
    ];
    for case in cases {
        let mut args = vec!["--capital", "600", "--price", "84000", "--json"]; // 本用例传给真实 CLI 的参数数组，显式保留输入/输出选择。
        args.extend_from_slice(case);
        let o = run(&args); // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
        assert!(!o.status.success(), "应拒绝 {case:?}");
        assert!(o.stdout.is_empty(), "失败时不应输出方案 {case:?}");
        assert!(!o.stderr.is_empty());
    }
}

/// 输入：无；返回：无；不能把不可行资金或收益强行变成两格。
#[test]
fn infeasible_plans_explain_the_constraint() {
    for extra in [
        vec!["--capital", "1"],
        vec!["--fee-pct", "30"],
        vec!["--min-order-usdt", "1000"],
        vec!["--min-notional", "1000"],
        vec!["--grids", "150"],
        vec!["--tick-size", "100000"],
        vec!["--max-qty", "0.00001"],
    ] {
        let mut args = vec!["--capital", "600", "--price", "84000", "--json"]; // 本用例传给真实 CLI 的参数数组，显式保留输入/输出选择。
        args.extend(extra);
        let o = run(&args); // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
        assert!(!o.status.success());
        assert!(o.stdout.is_empty());
    }
}

/// 输入：无；返回：无；配置覆盖、文件导出和未知字段校验走完整 CLI。
#[test]
fn config_overrides_and_export_are_consistent() {
    let dir = TempDir::new().unwrap(); // 自动清理的临时目录，测试不改用户配置或已有输出。
    let config = dir.path().join("grid.toml"); // 临时 TOML 路径，用于验证配置合并或相对 CSV 路径。
    let export = dir.path().join("plan.json"); // 临时 JSON 文件路径，核对文件与 stdout 字节一致。
    std::fs::write(&config, "capital = 600\nprice = 80000\nequity = 3000\n").unwrap();
    // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
    let o = run(&[
        "--config",
        config.to_str().unwrap(),
        "--price",
        "84000",
        "--json",
        "--output",
        export.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(o.stdout, std::fs::read(&export).unwrap());
    let p: Value = serde_json::from_slice(&o.stdout).unwrap(); // 真实 CLI 标准输出解码的方案，所有断言读取此结果。
    assert_eq!(p["reference_price"], 84000.0);
    check_invariants(&p);
    std::fs::write(&config, "capitla = 600\n").unwrap();
    assert!(
        !run(&["--config", config.to_str().unwrap()])
            .status
            .success()
    );
}

/// 输入：无；返回：无；交互问答生成 JSON 且输入结束时明确退出。
#[test]
fn interactive_input_keeps_json_clean_and_handles_eof() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_grid-planner")) // 可写标准输入的真实 CLI 子进程，用于交互输入/EOF 验收。
        .args(["--interactive", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"600\n84000\n600\n2\n")
        .unwrap();
    let o = child.wait_with_output().unwrap(); // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    check_invariants(&serde_json::from_slice(&o.stdout).unwrap());
    let o = Command::new(env!("CARGO_BIN_EXE_grid-planner")) // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
        .args(["--interactive", "--json"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!o.status.success());
}

/// 输入：临时目录和 K 线行数；返回：等幅且连续的日线 CSV 文件路径。
fn candles(dir: &TempDir, count: usize) -> std::path::PathBuf {
    let path = dir.path().join("candles.csv"); // 当前 CSV/请求路径，分别用于文件输入或公开端点路由。
    let mut csv = "open_time,open,high,low,close\n".to_owned(); // 临时五列 OHLC 文本缓冲，只用于确定性验收。
    for i in 0..count {
        csv.push_str(&format!(
            "{},84000,85000,83000,84000\n",
            1_700_000_000_000_u64 + i as u64 * 86_400_000
        ));
    }
    std::fs::write(&path, csv).unwrap();
    path
}

/// 输入：无；返回：无；ATR 使用已知 TR，并验证配置相对路径。
#[test]
fn atr_csv_and_manual_atr_agree() {
    let dir = TempDir::new().unwrap(); // 自动清理的临时目录，测试不改用户配置或已有输出。
    let path = candles(&dir, 20); // 当前 CSV/请求路径，分别用于文件输入或公开端点路由。
    let from_csv = plan(&["--mode", "atr", "--candles", path.to_str().unwrap()]); // 用户 CSV 入口的真实 CLI 结果，须与等值手填 ATR 相符。
    let manual = plan(&["--mode", "atr", "--atr", "2000"]); // 手填相同波幅的对照结果，验证 CSV ATR 链路。
    assert_eq!(from_csv["atr"], 2000.0);
    assert_eq!(from_csv["lower_price"], "78000.00");
    assert_eq!(from_csv["stop_loss"], "75000.00");
    assert_eq!(from_csv["grid_prices"], manual["grid_prices"]);
    check_invariants(&from_csv);
    let config = dir.path().join("grid.toml"); // 临时 TOML 路径，用于验证配置合并或相对 CSV 路径。
    std::fs::write(
        &config,
        "capital=600\nprice=84000\nmode='atr'\ncandles='candles.csv'\n",
    )
    .unwrap();
    let o = run(&["--config", config.to_str().unwrap(), "--json"]); // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

/// 输入：无；返回：无；拒绝不足、畸形、倒序、重复和缺失周期的 K 线。
#[test]
fn bad_candles_are_rejected() {
    let dir = TempDir::new().unwrap(); // 自动清理的临时目录，测试不改用户配置或已有输出。
    let path = candles(&dir, 3); // 当前 CSV/请求路径，分别用于文件输入或公开端点路由。
    assert!(
        !run(&[
            "--capital",
            "600",
            "--price",
            "84000",
            "--mode",
            "atr",
            "--candles",
            path.to_str().unwrap()
        ])
        .status
        .success()
    );
    for rows in [
        "1000,10,9,8,10\n",
        "1000,10,12,8,10\n1000,10,12,8,10\n",
        "2000,10,12,8,10\n1000,10,12,8,10\n",
        "1000,10,12,8,10\n2000,10,12,8,10\n",
        "1000,NaN,12,8,10\n",
        "1000,10,12\n",
    ] {
        std::fs::write(&path, format!("open_time,open,high,low,close\n{rows}")).unwrap();
        // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
        let o = run(&[
            "--capital",
            "600",
            "--price",
            "84000",
            "--mode",
            "atr",
            "--candles",
            path.to_str().unwrap(),
        ]);
        assert!(!o.status.success());
    }
}

/// 输入：无；返回：币安公开交易规则的确定性模拟响应。
fn exchange_info() -> Value {
    json!({"symbols": [{"symbol": "BTCUSDT", "status": "TRADING", "quoteAsset": "USDT", "isSpotTradingAllowed": true,
    "filters": [
        {"filterType": "PRICE_FILTER", "tickSize": "0.10", "minPrice": "0.10", "maxPrice": "1000000"},
        {"filterType": "LOT_SIZE", "stepSize": "0.00001", "minQty": "0.00001", "maxQty": "100"},
        {"filterType": "MIN_NOTIONAL", "minNotional": "10"},
        {"filterType": "NOTIONAL", "minNotional": "15", "maxNotional": "100000"},
        {"filterType": "MAX_NUM_ORDERS", "maxNumOrders": 150}
    ]}]})
}

/// 输入：请求 URL；返回：对应的行情响应，末根未收盘 K 线故意含极端高价。
fn market_response(path: &str) -> Value {
    if path.starts_with("/api/v3/ticker/price") {
        return json!({"symbol": "BTCUSDT", "price": "84000"});
    }
    if path.starts_with("/api/v3/time") {
        return json!({"serverTime": 2_000_000_000_000_u64});
    }
    if path.starts_with("/api/v3/exchangeInfo") {
        return exchange_info();
    }
    let rows: Vec<_> = (0..=100) // 确定性历史 K 线行，含已收盘样本及刻意设计的边界输入。
        .map(|i| {
            let start = 2_000_000_000_000_u64 - 100 * 86_400_000 + i * 86_400_000; // 模拟样本 Unix 开盘毫秒，与固定日线跨度配对。
            json!([
                start,
                "84000",
                if i == 100 { "999999" } else { "85000" },
                "83000",
                "84000",
                "10",
                start + 86_400_000 - 1
            ])
        })
        .collect();
    json!(rows)
}

/// 输入：响应回调及预期请求数；返回：本地服务器 URL 和线程句柄。
fn server(respond: fn(&str) -> (u16, Value), requests: usize) -> (String, JoinHandle<()>) {
    // 本地服务线程句柄，join 保证请求断言与清理已结束。
    let listener = TcpListener::bind("127.0.0.1:0").unwrap(); // 只绑定回环地址的临时服务，随机端口避免用例冲突。
    let url = format!("http://{}", listener.local_addr().unwrap()); // 本地模拟 API/代理地址或查询解析结果，不访问交易账户。
    let handle = thread::spawn(move || {
        for _ in 0..requests {
            let (mut stream, _) = listener.accept().unwrap(); // 当前客户端连接，服务线程逐个读取并响应。
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut buf = [0; 4096]; // 当前 HTTP 请求读取缓冲，服务只为测试所需请求工作。
            let len = stream.read(&mut buf).unwrap(); // 实际读取字节数，不将未使用缓冲尾部当成请求。
            let request = String::from_utf8_lossy(&buf[..len]); // 当前 HTTP 请求文本，检查公开路径及认证信息。
            assert!(request.starts_with("GET "));
            assert!(!request.to_lowercase().contains("x-mbx-apikey"));
            let path = request.split_whitespace().nth(1).unwrap(); // 当前 CSV/请求路径，分别用于文件输入或公开端点路由。
            let (status, value) = respond(path); // 用例控制 HTTP 状态及 JSON，覆盖成功与错误响应。
            let body = value.to_string(); // 模拟响应的 JSON 文本，Content-Length 按字节长度计算。
            write!(stream, "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (url, handle)
}

/// 输入：请求路径；返回：正常公开市场响应。
fn ok_response(path: &str) -> (u16, Value) {
    (200, market_response(path))
}

/// 输入：临时目录、根数及场景；返回：仅供 E2E 的连续震荡/下跌/跳空日线 CSV。
fn adaptive_candles(dir: &TempDir, count: usize, scenario: &str) -> std::path::PathBuf {
    let path = dir.path().join(format!("adaptive-{scenario}.csv"));
    let mut csv = "open_time,open,high,low,close\n".to_owned();
    let mut previous = 84000.0_f64;
    for i in 0..count {
        let wave = 3000.0 * (i as f64 * std::f64::consts::TAU / 12.0).sin();
        let close = match scenario {
            "flat" => 84000.0,
            "down" => 84000.0 - i as f64 * 180.0,
            "gap" if i >= count * 4 / 5 => 50000.0,
            "tail" if i >= count * 4 / 5 => 84000.0 + wave * 0.8,
            _ => 84000.0 + wave,
        };
        let open = if scenario == "gap" && i == count * 4 / 5 {
            close
        } else {
            previous
        };
        csv.push_str(&format!(
            "{},{open},{},{},{close}\n",
            1_700_000_000_000_u64 + i as u64 * 86_400_000,
            open.max(close) + 1200.0,
            open.min(close) - 1200.0
        ));
        previous = close;
    }
    std::fs::write(&path, csv).unwrap();
    path
}

/// 输入：CSV 路径与额外参数；返回：真实 CLI 的自适应方案，复用既有 JSON 及资金断言。
fn adaptive_plan(path: &std::path::Path, extra: &[&str]) -> Value {
    let mut args = vec![
        "--algorithm",
        "adaptive",
        "--mode",
        "atr",
        "--candles",
        path.to_str().unwrap(),
        "--capital",
        "3000",
        "--equity",
        "15000",
        "--min-order-usdt",
        "10",
        "--max-grids",
        "24",
    ];
    args.extend_from_slice(extra);
    let p = plan(&args);
    check_invariants(&p);
    p
}

/// 输入：无；返回：无；新版具有旧版对照、独立最终检验和可复算时间界限。
#[test]
fn adaptive_compares_costed_returns_without_relaxing_constraints() {
    let dir = TempDir::new().unwrap();
    let p = adaptive_plan(&adaptive_candles(&dir, 180, "wave"), &[]);
    assert_eq!(p["algorithm"], "adaptive");
    let o = &p["optimization"];
    assert_eq!(o["development_folds"].as_array().unwrap().len(), 3);
    assert_eq!(o["holdout"]["evaluated_bars"], 36);
    assert!(o["feasible_candidates"].as_u64().unwrap() > 1);
    assert!(
        o["development_score"].as_f64().unwrap() + 1e-9 >= o["baseline_score"].as_f64().unwrap()
    );
    assert!(
        o["holdout"]["candidate"]["trading_costs_usdt"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert!(
        o["holdout"]["candidate"]["net_return_pct"]
            .as_f64()
            .unwrap()
            .is_finite()
    );
    assert!(
        o["holdout"]["candidate"]["stress_net_return_pct"]
            .as_f64()
            .unwrap()
            <= o["holdout"]["candidate"]["net_return_pct"]
                .as_f64()
                .unwrap()
                + 1e-9
    );
    let last = &o["development_folds"][2];
    assert!(
        last["evaluation_end_open_ms"].as_u64().unwrap()
            < o["holdout"]["evaluation_start_open_ms"].as_u64().unwrap()
    );
}

/// 输入：无；返回：无；修改未参与选参的尾部不会改变参数或发展段指标。
#[test]
fn adaptive_holdout_cannot_influence_parameter_selection() {
    let dir = TempDir::new().unwrap();
    let a = adaptive_plan(&adaptive_candles(&dir, 180, "wave"), &[]);
    let b = adaptive_plan(&adaptive_candles(&dir, 180, "tail"), &[]);
    for field in [
        "selected_range_atr_mult",
        "selected_grids",
        "development_score",
        "development_folds",
    ] {
        assert_eq!(
            a["optimization"][field], b["optimization"][field],
            "{field}"
        );
    }
    assert_ne!(a["optimization"]["holdout"], b["optimization"]["holdout"]);
}

/// 输入：无；返回：无；固定格数被每段验证及最终方案尊重，不被自动替换。
#[test]
fn adaptive_preserves_explicit_grid_count() {
    let dir = TempDir::new().unwrap();
    let p = adaptive_plan(&adaptive_candles(&dir, 180, "wave"), &["--grids", "4"]);
    assert_eq!(p["grid_count"], 4);
    assert_eq!(p["optimization"]["selected_grids"], 4);
}

/// 输入：无；返回：无；单边下跌输出观望、库存亏损及原因。
#[test]
fn adaptive_declining_market_does_not_claim_profit() {
    let dir = TempDir::new().unwrap();
    let p = adaptive_plan(&adaptive_candles(&dir, 180, "down"), &["--price", "51780"]);
    assert_eq!(p["optimization"]["recommendation"], "wait");
    assert!(!p["optimization"]["reason"].as_str().unwrap().is_empty());
    assert!(
        p["optimization"]["holdout"]["candidate"]["net_profit_usdt"]
            .as_f64()
            .unwrap()
            < 0.0
    );
}

/// 输入：无；返回：无；跳空止损不会伪造按 SL 成交或忽略库存亏损。
#[test]
fn adaptive_gap_exit_can_exceed_the_scenario_budget() {
    let dir = TempDir::new().unwrap();
    let p = adaptive_plan(&adaptive_candles(&dir, 180, "gap"), &[]);
    let metrics = &p["optimization"]["holdout"]["candidate"];
    assert_eq!(metrics["stop_triggered"], true);
    assert!(metrics["net_profit_usdt"].as_f64().unwrap() < -300.0);
    assert_eq!(p["optimization"]["recommendation"], "wait");
}

/// 输入：无；返回：无；无成交时双边建仓/清仓成本按完整库存独立核算，并建议观望。
#[test]
fn adaptive_no_cycles_accounts_for_initial_and_exit_costs() {
    let dir = TempDir::new().unwrap();
    let p = adaptive_plan(&adaptive_candles(&dir, 180, "flat"), &["--max-grids", "2"]);
    let m = &p["optimization"]["holdout"]["candidate"];
    let base = p["initial_base_quantity_estimate"].as_f64().unwrap();
    let cost = p["effective_cost_per_side_pct"].as_f64().unwrap() / 100.0;
    let loss = base * 84000.0 * cost * 2.0;
    assert_eq!(m["completed_cycles"], 0);
    assert!((m["net_profit_usdt"].as_f64().unwrap() + loss).abs() < 1e-7);
    assert!((m["trading_costs_usdt"].as_f64().unwrap() - loss).abs() < 1e-7);
    assert_eq!(p["optimization"]["recommendation"], "wait");
}

/// 输入：公开请求路径；返回：180 根震荡日线与未收盘极端线，复用既有过滤器。
fn adaptive_response(path: &str) -> (u16, Value) {
    if !path.starts_with("/api/v3/klines") {
        return ok_response(path);
    }
    let rows: Vec<_> = (0..=180)
        .map(|i| {
            let start = 2_000_000_000_000_u64 - (180 - i) * 86_400_000;
            let open = 84000.0 + 3000.0 * ((i as f64 - 1.0) * std::f64::consts::TAU / 12.0).sin();
            let close = 84000.0 + 3000.0 * (i as f64 * std::f64::consts::TAU / 12.0).sin();
            json!([
                start,
                open.to_string(),
                if i == 180 {
                    "999999".into()
                } else {
                    (open.max(close) + 1200.0).to_string()
                },
                (open.min(close) - 1200.0).to_string(),
                close.to_string(),
                "10",
                start + 86_400_000 - 1
            ])
        })
        .collect();
    (200, json!(rows))
}

/// 输入：无；返回：无；真实 HTTP 历史驱动自适应算法，未收盘线不能污染选参。
#[test]
fn adaptive_live_uses_only_the_attached_closed_history() {
    let (url, handle) = server(adaptive_response, 4);
    let o = run(&[
        "--capital",
        "3000",
        "--equity",
        "15000",
        "--live",
        "--mode",
        "atr",
        "--algorithm",
        "adaptive",
        "--history-bars",
        "180",
        "--max-grids",
        "24",
        "--min-order-usdt",
        "10",
        "--api-base-url",
        &url,
        "--json",
    ]);
    handle.join().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let p: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(p["closed_candle_count"], 180);
    assert_eq!(p["history"]["closed_candle_count"], 180);
    assert_eq!(p["optimization"]["holdout"]["evaluated_bars"], 36);
    assert!(
        p["history"]["candles"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["high"].as_f64().unwrap() < 90000.0)
    );
    check_invariants(&p);
}

/// 输入：无；返回：无；缺历史、短历史、绝对停止价或非 ATR 模式明确失败。
#[test]
fn adaptive_rejects_unsupported_or_insufficient_evidence() {
    let dir = TempDir::new().unwrap();
    let short = adaptive_candles(&dir, 60, "short");
    let full = adaptive_candles(&dir, 180, "wave");
    let cases = [
        vec!["--mode", "percent"],
        vec!["--mode", "atr", "--atr", "2000"],
        vec!["--mode", "atr", "--candles", short.to_str().unwrap()],
        vec![
            "--mode",
            "atr",
            "--candles",
            full.to_str().unwrap(),
            "--stop-loss",
            "50000",
        ],
        vec![
            "--mode",
            "atr",
            "--candles",
            full.to_str().unwrap(),
            "--take-profit",
            "100000",
        ],
    ];
    for extra in cases {
        let mut args = vec![
            "--capital",
            "3000",
            "--price",
            "84000",
            "--algorithm",
            "adaptive",
            "--json",
        ];
        args.extend(extra);
        let o = run(&args);
        assert!(!o.status.success());
        assert!(o.stdout.is_empty());
        assert!(String::from_utf8_lossy(&o.stderr).contains("自适应"));
    }
}

/// 输入：无；返回：无；自适应 TOML/CLI 覆盖及 JSON 文件和 stdout 字节一致。
#[test]
fn adaptive_config_export_preserves_the_full_audit() {
    let dir = TempDir::new().unwrap();
    adaptive_candles(&dir, 180, "wave");
    let config = dir.path().join("adaptive.toml");
    let output = dir.path().join("adaptive.json");
    std::fs::write(&config, "capital=3000\nequity=15000\nprice=84000\nmode='atr'\nalgorithm='classic'\ncandles='adaptive-wave.csv'\nmax_grids=24\nmin_order_usdt=10\n").unwrap();
    let o = run(&[
        "--config",
        config.to_str().unwrap(),
        "--algorithm",
        "adaptive",
        "--json",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read(output).unwrap(), o.stdout);
    let p: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(p["algorithm"], "adaptive");
    assert!(p["optimization"]["holdout"]["baseline"].is_object());
}

/// 输入：请求路径（忽略）；返回：HTTP 429 响应。
fn rate_limited(_: &str) -> (u16, Value) {
    (429, json!({"msg": "Too many requests"}))
}

/// 输入：请求路径（忽略）；返回：缺少必要过滤器的交易规则。
fn missing_filters(_: &str) -> (u16, Value) {
    (
        200,
        json!({"symbols": [{"symbol": "BTCUSDT", "status": "TRADING", "quoteAsset": "USDT", "isSpotTradingAllowed": true, "filters": []}]}),
    )
}

/// 输入：无；返回：无；真实 HTTP + CLI 路径排除未收盘线并读取全部过滤器。
#[test]
fn live_mode_uses_rules_and_closed_candles() {
    // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
    let (url, handle) = server(ok_response, 4); // 地址交给 CLI，线程句柄用于等待模拟服务断言完成。
    let o = run(&[
        "--capital",
        "600",
        "--live",
        "--mode",
        "atr",
        "--api-base-url",
        &url,
        "--json",
    ]);
    handle.join().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let p: Value = serde_json::from_slice(&o.stdout).unwrap(); // 真实 CLI 标准输出解码的方案，所有断言读取此结果。
    assert_eq!(p["atr"], 2000.0);
    assert_eq!(p["closed_candle_count"], 100);
    assert_eq!(p["rules"]["min_notional"], 15.0);
    assert_eq!(p["rules"]["tick_size"], "0.10");
    assert_eq!(p["data_source"], "binance_public_api");
    check_invariants(&p);
}

/// 输入：无；返回：无；API 失败时不能偷偷退回默认规则。
#[test]
fn api_errors_do_not_fall_back_to_offline_assumptions() {
    for respond in [rate_limited as fn(&str) -> (u16, Value), missing_filters] {
        // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
        let (url, handle) = server(respond, 1); // 地址交给 CLI，线程句柄用于等待模拟服务断言完成。
        let o = run(&[
            "--capital",
            "600",
            "--live",
            "--api-base-url",
            &url,
            "--json",
        ]);
        handle.join().unwrap();
        assert!(!o.status.success());
        assert!(o.stdout.is_empty());
    }
}

/// 输入：无；返回：无；代理地址非法时不能启动计算或输出成功方案。
#[test]
fn invalid_proxy_addresses_are_rejected() {
    for proxy in [
        "127.0.0.1:7890",
        "ftp://127.0.0.1:21",
        "http://127.0.0.1:0",
        "http://127.0.0.1:99999",
        "http://127.0.0.1:8080/path",
        "http://user:pass@127.0.0.1:8080",
    ] {
        // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
        let o = run(&[
            "--capital",
            "600",
            "--price",
            "84000",
            "--proxy-url",
            proxy,
            "--json",
        ]);
        assert!(!o.status.success(), "应拒绝代理 {proxy}");
        assert!(o.stdout.is_empty());
    }
}

/// 输入：已连接的 SOCKS5 流；返回：无；确认代理接收域名而不是本机 DNS 结果。
fn socks_handshake(stream: &mut std::net::TcpStream) {
    let mut hello = [0; 2]; // SOCKS5 握手头，包含协议版本与认证方法数量。
    stream.read_exact(&mut hello).unwrap();
    assert_eq!(hello[0], 5);
    let mut methods = vec![0; hello[1] as usize]; // 客户端支持的 SOCKS5 认证方法，验收要求无需账号。
    stream.read_exact(&mut methods).unwrap();
    assert!(methods.contains(&0));
    stream.write_all(&[5, 0]).unwrap();
    let mut request = [0; 5]; // SOCKS5 CONNECT 请求头，包含目标地址类型及域名长度。
    stream.read_exact(&mut request).unwrap();
    assert_eq!(&request[..4], &[5, 1, 0, 3], "socks5h 应让代理解析域名");
    let mut host = vec![0; request[4] as usize]; // SOCKS5 请求中的远程目标域名，验证 socks5h 没有本地 DNS 回退。
    stream.read_exact(&mut host).unwrap();
    assert_eq!(host, b"market.invalid");
    let mut port = [0; 2]; // SOCKS5 目标端口的两个网络序字节，应指向 HTTP 80。
    stream.read_exact(&mut port).unwrap();
    assert_eq!(u16::from_be_bytes(port), 80);
    stream
        .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 80])
        .unwrap();
}

/// 输入：代理类型；返回：本地代理 URL 和线程；依次承载所有三个公开行情 GET。
fn proxy_server(socks: bool, requests: usize) -> (String, JoinHandle<()>) {
    // 本地服务线程句柄，join 保证请求断言与清理已结束。
    let listener = TcpListener::bind("127.0.0.1:0").unwrap(); // 只绑定回环地址的临时服务，随机端口避免用例冲突。
    let scheme = if socks { "socks5h" } else { "http" }; // 当前测试协议，分别验收 HTTP 与 SOCKS5h 的传输路径。
    let url = format!("{scheme}://{}", listener.local_addr().unwrap()); // 本地模拟 API/代理地址或查询解析结果，不访问交易账户。
    let handle = thread::spawn(move || {
        for _ in 0..requests {
            let (mut stream, _) = listener.accept().unwrap(); // 当前客户端连接，服务线程逐个读取并响应。
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            if socks {
                socks_handshake(&mut stream);
            }
            let mut bytes = [0; 4096]; // 代理接收的 HTTP/CONNECT 原始字节缓冲。
            let len = stream.read(&mut bytes).unwrap(); // 实际读取字节数，不将未使用缓冲尾部当成请求。
            let request = String::from_utf8_lossy(&bytes[..len]); // 当前 HTTP 请求文本，检查公开路径及认证信息。
            assert!(request.starts_with("GET "));
            assert!(!request.to_lowercase().contains("x-mbx-apikey"));
            // 当前 CSV/请求路径，分别用于文件输入或公开端点路由。
            let target = request.split_whitespace().nth(1).unwrap(); // 请求首行的目标，HTTP 代理用绝对 URL，SOCKS 隧道用相对路径。
            let path = if socks {
                target.to_owned()
            } else {
                assert!(target.starts_with("http://market.invalid/"));
                reqwest::Url::parse(target).unwrap().path().to_owned()
            };
            let body = market_response(&path).to_string(); // 模拟响应的 JSON 文本，Content-Length 按字节长度计算。
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (url, handle)
}

/// 输入：无；返回：无；真实 CLI 通过 HTTP 和 SOCKS5h 代理获取数据，目标域名无法直连。
#[test]
fn public_requests_follow_explicit_http_and_socks_proxies() {
    for socks in [false, true] {
        // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
        let (proxy, handle) = proxy_server(socks, 3); // 地址交给 CLI，线程句柄用于等待模拟服务断言完成。
        let o = run(&[
            "--capital",
            "600",
            "--live",
            "--api-base-url",
            "http://market.invalid",
            "--proxy-url",
            &proxy,
            "--json",
        ]);
        handle.join().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let p: Value = serde_json::from_slice(&o.stdout).unwrap(); // 真实 CLI 标准输出解码的方案，所有断言读取此结果。
        check_invariants(&p);
    }
}

/// 输入：无；返回：无；HTTPS 使用 HTTP CONNECT，拒绝代理后明确失败。
#[test]
fn https_proxy_uses_connect_and_does_not_hide_failure() {
    // 本地服务线程句柄，join 保证请求断言与清理已结束。
    let listener = TcpListener::bind("127.0.0.1:0").unwrap(); // 只绑定回环地址的临时服务，随机端口避免用例冲突。
    let proxy = format!("http://{}", listener.local_addr().unwrap()); // 本地代理地址；目标域名故意不可直连，成功证明代理路径生效。
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap(); // 当前 CONNECT/HTTP 客户端连接。
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = [0; 2048]; // 代理接收的 HTTP/CONNECT 原始字节缓冲。
        let len = stream.read(&mut bytes).unwrap(); // 实际读取字节数，不将未使用缓冲尾部当成请求。
        let request = String::from_utf8_lossy(&bytes[..len]); // 当前 HTTP 请求文本，检查公开路径及认证信息。
        assert!(request.starts_with("CONNECT market.invalid:443 "));
        stream
            .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });
    // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
    let o = run(&[
        "--capital",
        "600",
        "--live",
        "--api-base-url",
        "https://market.invalid",
        "--proxy-url",
        &proxy,
        "--json",
    ]);
    handle.join().unwrap();
    assert!(!o.status.success());
    assert!(o.stdout.is_empty());
}

/// 输入：请求路径；返回：历史模拟响应，同时确认根数和已收盘请求截止时间。
fn history_response(path: &str) -> (u16, Value) {
    if !path.starts_with("/api/v3/klines") {
        return ok_response(path);
    }
    let url = reqwest::Url::parse(&format!("http://local{path}")).unwrap(); // 本地模拟 API/代理地址或查询解析结果，不访问交易账户。
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect(); // 历史请求查询参数，核验 symbol/interval/limit/endTime。
    let count: u64 = query["limit"].parse().unwrap(); // 客户端实际请求根数，分别验收上限和短历史。
    assert_eq!(query["symbol"], "BTCUSDT");
    assert_eq!(query["interval"], "1d");
    assert_eq!(
        query["endTime"],
        (2_000_000_000_000_u64 / 86_400_000 * 86_400_000 - 1).to_string()
    );
    if count != 1000 {
        return ok_response(path);
    }
    let rows: Vec<_> = (0..1000) // 确定性历史 K 线行，含已收盘样本及刻意设计的边界输入。
        .map(|i| {
            let start = 2_000_000_000_000_u64 - (1000 - i) * 86_400_000; // 模拟样本 Unix 开盘毫秒，与固定日线跨度配对。
            json!([
                start,
                "84000",
                "85000",
                "83000",
                "84000",
                "10",
                start + 86_400_000 - 1
            ])
        })
        .collect();
    (200, json!(rows))
}

/// 输入：无；返回：无；历史获取不需要本金，导出仅包含真实请求中的已收盘数据。
#[test]
fn historical_fetch_is_independent_and_exports_closed_ohlc() {
    // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
    let dir = TempDir::new().unwrap(); // 自动清理的临时目录，测试不改用户配置或已有输出。
    let output = dir.path().join("history.json"); // 该用例独立的历史导出文件，不覆盖用户文件。
    let (url, handle) = server(history_response, 4); // 地址交给 CLI，线程句柄用于等待模拟服务断言完成。
    let o = run(&[
        "--fetch-history",
        "--history-bars",
        "30",
        "--api-base-url",
        &url,
        "--output",
        output.to_str().unwrap(),
    ]);
    handle.join().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(o.stdout, std::fs::read(output).unwrap());
    let h: Value = serde_json::from_slice(&o.stdout).unwrap(); // 真实历史 JSON，核对 OHLC、ATR、根数及来源时间。
    assert_eq!(h["symbol"], "BTCUSDT");
    assert_eq!(h["requested_candle_count"], 30);
    assert_eq!(h["closed_candle_count"], 30);
    assert_eq!(h["candles"].as_array().unwrap().len(), 30);
    assert_eq!(h["atr"], 2000.0);
    assert_eq!(h["current_price"], 84000.0);
    assert_eq!(h["market_as_of_ms"], 2_000_000_000_000_u64);
    assert_eq!(
        h["first_candle_open_ms"],
        2_000_000_000_000_u64 - 30 * 86_400_000
    );
    assert!(h["last_candle_close_ms"].as_u64().unwrap() < 2_000_000_000_000);
    assert!(h["source_url"].as_str().unwrap().starts_with(&url));
    assert!(
        h["candles"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["high"] == 85000.0)
    );
}

/// 输入：无；返回：无；1000 根上限及短历史都按实际数量报告，不凑数。
#[test]
fn historical_counts_are_honest_at_api_limits() {
    for (requested, actual) in [(1000, 1000), (180, 100)] {
        // 逐项运行预设的响应/根数边界，不访问真实行情。
        // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
        let (url, handle) = server(history_response, 4); // 地址交给 CLI，线程句柄用于等待模拟服务断言完成。
        let o = run(&[
            "--fetch-history",
            "--history-bars",
            &requested.to_string(),
            "--api-base-url",
            &url,
        ]);
        handle.join().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let h: Value = serde_json::from_slice(&o.stdout).unwrap(); // 真实历史 JSON，核对 OHLC、ATR、根数及来源时间。
        assert_eq!(h["requested_candle_count"], requested);
        assert_eq!(h["closed_candle_count"], actual);
        assert_eq!(h["candles"].as_array().unwrap().len(), actual as usize);
        assert_eq!(
            h["warnings"].as_array().unwrap().is_empty(),
            requested == actual
        );
    }
}

/// 输入：无；返回：无；非法历史参数和本地数据混用在联网前失败。
#[test]
fn invalid_history_inputs_do_not_use_local_data() {
    for extra in [
        vec!["--history-bars", "0"],
        vec!["--history-bars", "14"],
        vec!["--history-bars", "1001"],
        vec!["--atr-period", "201"],
        vec!["--interval", "1M"],
        vec!["--candles", "examples/candles-demo.csv"],
        vec!["--atr", "2000"],
        vec!["--symbol", "btcusdt"],
        vec!["--proxy-url", "127.0.0.1:7890"],
    ] {
        let mut args = vec!["--fetch-history", "--api-base-url", "http://127.0.0.1:1"]; // 本用例传给真实 CLI 的参数数组，显式保留输入/输出选择。
        args.extend(extra);
        let o = run(&args); // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
        assert!(!o.status.success());
        assert!(o.stdout.is_empty());
        assert!(
            !String::from_utf8_lossy(&o.stderr).contains("读取公开 API"),
            "应先校验历史输入"
        );
    }
    // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
    let o = run(&[
        "--capital",
        "600",
        "--price",
        "84000",
        "--history-bars",
        "30",
    ]);
    assert!(!o.status.success(), "离线生成不能悄悄接受历史联网参数");
}

/// 输入：请求路径；返回：样本不足的历史响应。
fn short_history(path: &str) -> (u16, Value) {
    let mut value = market_response(path); // 本用例故意缩短或打断连续性的响应，验证失败时不输出旧数据。
    if path.starts_with("/api/v3/klines") {
        value.as_array_mut().unwrap().truncate(10);
    }
    (200, value)
}

/// 输入：请求路径；返回：漏根的历史响应。
fn gapped_history(path: &str) -> (u16, Value) {
    let mut value = market_response(path); // 本用例故意缩短或打断连续性的响应，验证失败时不输出旧数据。
    if path.starts_with("/api/v3/klines") {
        value.as_array_mut().unwrap().remove(1);
    }
    (200, value)
}

/// 输入：无；返回：无；历史数据不足、不连续或 HTTP 出错不能输出旧数据。
#[test]
fn historical_failures_do_not_fall_back() {
    for (respond, requests) in [
        // 逐项运行预设的响应/根数边界，不访问真实行情。
        (short_history as fn(&str) -> (u16, Value), 4),
        (gapped_history, 4),
        (rate_limited, 1),
    ] {
        // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
        let (url, handle) = server(respond, requests); // 地址交给 CLI，线程句柄用于等待模拟服务断言完成。
        let o = run(&[
            "--fetch-history",
            "--history-bars",
            "30",
            "--api-base-url",
            &url,
        ]);
        handle.join().unwrap();
        assert!(!o.status.success());
        assert!(o.stdout.is_empty());
    }
}

/// 输入：无；返回：无；联网方案的 ATR 和历史快照来自同一批已收盘样本。
#[test]
fn live_plan_includes_selected_historical_snapshot() {
    // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
    let (url, handle) = server(history_response, 4); // 地址交给 CLI，线程句柄用于等待模拟服务断言完成。
    let o = run(&[
        "--capital",
        "600",
        "--live",
        "--mode",
        "atr",
        "--history-bars",
        "30",
        "--api-base-url",
        &url,
        "--json",
    ]);
    handle.join().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let p: Value = serde_json::from_slice(&o.stdout).unwrap(); // 真实 CLI 标准输出解码的方案，所有断言读取此结果。
    assert_eq!(p["history"]["closed_candle_count"], 30);
    assert_eq!(p["history"]["atr"], p["atr"]);
    assert_eq!(p["closed_candle_count"], 30);
    assert_eq!(p["lower_price"], "78000.00");
    check_invariants(&p);
}

/// 输入：无；返回：无；历史请求经 HTTP / SOCKS5h 代理访问无法直连的域名。
#[test]
fn historical_requests_follow_the_configured_proxy() {
    for socks in [false, true] {
        // 真实进程退出状态、stdout、stderr，联合验证成功与失败路径。
        let (proxy, handle) = proxy_server(socks, 4); // 地址交给 CLI，线程句柄用于等待模拟服务断言完成。
        let o = run(&[
            "--fetch-history",
            "--history-bars",
            "30",
            "--api-base-url",
            "http://market.invalid",
            "--proxy-url",
            &proxy,
        ]);
        handle.join().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let h: Value = serde_json::from_slice(&o.stdout).unwrap(); // 真实历史 JSON，核对 OHLC、ATR、根数及来源时间。
        assert_eq!(h["closed_candle_count"], 30);
        assert_eq!(h["atr"], 2000.0);
    }
}
