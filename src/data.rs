//! 数据层：只读取公开市场接口；显式选择离线来源，不用示例行情掩盖联网失败。

use crate::candles::{Candle, read_csv, validate_series, wilder_atr};
use crate::config::{RangeMode, Settings, interval_ms, positive};
use crate::model::{History, Market, Rules};
use anyhow::{Context, Result, ensure};
use reqwest::blocking::Client;
use serde_json::Value;
use std::time::Duration;

/// 输入：设置、联网标记与 API 地址；返回：带数据来源的价格、波动率和过滤器。
pub fn load(settings: &Settings, live: bool, base_url: &str) -> Result<Market> {
    if live {
        return load_live(settings, base_url);
    }
    // 尚可补充 ATR/历史的本次行情快照；不混用之前请求结果。
    let mut market = Market {
        price: settings.price,
        atr: settings.atr,
        rules: Rules::offline(settings),
        data_source: "offline_inputs".into(),
        atr_source: settings.atr.map(|_| "manual_atr".into()),
        closed_candle_count: 0,
        last_candle_open_ms: None,
        market_as_of_ms: None,
        history: None,
    };
    if let Some(path) = &settings.candles {
        // 仅用户明确指定时读取本地 CSV；path 是输入文件路径。
        let candles = read_csv(path)?; // 本次已校验的 OHLC 样本，后续 ATR 与历史展示复用同一批数据。
        validate_series(&candles, &settings.interval)?;
        let now = std::time::SystemTime::now() // 离线使用本机时间，联网使用服务器时间；单位为 Unix 毫秒。
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis();
        let step = interval_ms(&settings.interval)?; // 当前 K 线固定周期的毫秒数，用于确定收盘与连续性。
        ensure!(
            candles
                .last()
                .is_none_or(|c| u128::from(c.open_time) + u128::from(step) <= now),
            "CSV 包含尚未收盘的 K 线，请先移除末根"
        );
        attach_atr(&mut market, &candles, settings, "csv_wilder_atr")?;
    }
    Ok(market)
}

/// 输入：设置与 API 地址；返回：公开市场快照；网络失败时不会使用离线规则代替。
fn load_live(settings: &Settings, base_url: &str) -> Result<Market> {
    let client = build_client(settings)?; // 全流程复用的客户端，统一超时与显式代理设置。
    let symbol = [("symbol", settings.symbol.clone())]; // 仅含交易对的公开查询参数，不含密钥或签名。
    let info = get_json(&client, base_url, "exchangeInfo", &symbol)?; // exchangeInfo 原始 JSON，先验证交易状态与过滤器。
    let rules = read_rules(&info, &settings.symbol)?; // 读取/组装的规则快照；必需过滤器缺失就报错。
    let ticker = get_json(&client, base_url, "ticker/price", &symbol)?; // 公开最新报价 JSON，与目标交易对再次核对。
    ensure!(
        ticker["symbol"].as_str() == Some(settings.symbol.as_str()),
        "行情返回交易对不匹配"
    );
    let price = decimal_field(&ticker, "price")?; // 公开最新报价，单位 USDT/基础币，不用示例价格替代。
    positive("API 现价", price)?;
    // 尚可补充 ATR/历史的本次行情快照；不混用之前请求结果。
    let time = get_json(&client, base_url, "time", &[])?; // 公开服务器时间响应，用于统一历史收盘判定。
    let now = time["serverTime"].as_u64().context("缺少合法 serverTime")?; // 离线使用本机时间，联网使用服务器时间；单位为 Unix 毫秒。
    let mut market = Market {
        price,
        atr: None,
        rules,
        data_source: "binance_public_api".into(),
        atr_source: None,
        closed_candle_count: 0,
        last_candle_open_ms: None,
        market_as_of_ms: Some(now),
        history: None,
    };
    if settings.mode == RangeMode::Atr || settings.history_bars.is_some() {
        let count = settings // 请求根数；未指定时至少取 100 根，留出 Wilder 平滑的预热样本。
            .history_bars
            .unwrap_or((settings.atr_period * 5 + 1).clamp(100, 1000));
        let candles = request_candles(&client, base_url, settings, now, count)?; // 本次已校验的 OHLC 样本，后续 ATR 与历史展示复用同一批数据。
        attach_atr(
            &mut market,
            &candles,
            settings,
            "binance_closed_klines_wilder_atr",
        )?;
        market.history = Some(History::new(settings, &market, base_url, candles, count)?);
    }
    Ok(market)
}

/// 输入：客户端、API 地址、设置、服务器时间与根数；返回：最近的连续已收盘 K 线。
fn request_candles(
    client: &Client,
    base: &str,
    settings: &Settings,
    now: u64,
    count: usize,
) -> Result<Vec<Candle>> {
    // Binance 默认 UTC；周线从周一开始，Unix 起点是周四，因此周线原点偏移四天。
    let step = interval_ms(&settings.interval)?; // 当前 K 线固定周期的毫秒数，用于确定收盘与连续性。
    // UTC 周线从周一开始的原点偏移；其余固定周期以 Unix 起点对齐。
    let origin = if settings.interval == "1w" {
        4 * 86_400_000
    } else {
        0
    };
    let open = now.checked_sub(origin).context("服务器时间早于 K 线原点")? / step * step + origin; // 服务器时间所在的当前未收盘 K 线的开盘毫秒。
    let end = open // 当前开盘毫秒减一，只请求上一根及更早的已收盘数据。
        .checked_sub(1)
        .context("服务器时间无法确定已收盘 K 线")?;
    // 明确指定交易对、周期、数量与截止时间，避免包含当前未收盘 K 线。
    let params = [
        ("symbol", settings.symbol.clone()),
        ("interval", settings.interval.clone()),
        ("limit", count.to_string()),
        ("endTime", end.to_string()),
    ];
    let response = get_json(client, base, "klines", &params)?; // 公开端点原始响应，HTTP 失败或非法 JSON 不做离线回退。
    let candles = closed_klines(&response, now, &settings.interval)?; // 本次已校验的 OHLC 样本，后续 ATR 与历史展示复用同一批数据。
    let skip = candles.len().saturating_sub(count); // API 多返回样本时从前部裁掉的根数，保留最近 count 根。
    Ok(candles.into_iter().skip(skip).collect())
}

/// 输入：设置中的可选代理；返回：用于所有公开请求的客户端，显式代理不会直连回退。
fn build_client(settings: &Settings) -> Result<Client> {
    let mut builder = Client::builder() // 连接/总超时受限的 HTTP 构建器，显式代理时清除环境代理规则。
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .user_agent(concat!("grid-planner/", env!("CARGO_PKG_VERSION")));
    if let Some(proxy) = &settings.proxy_url {
        // 显式代理覆盖环境代理，失败时不尝试直连。
        builder = builder
            .no_proxy()
            .proxy(reqwest::Proxy::all(proxy).context("无法配置代理")?);
    }
    builder.build().context("无法初始化网络客户端")
}

/// 输入：HTTP 客户端、地址、公开端点和查询参数；返回：成功解析的 JSON 或 HTTP 错误。
fn get_json(
    client: &Client,
    base: &str,
    endpoint: &str,
    query: &[(&str, String)],
) -> Result<Value> {
    let url = format!("{}/api/v3/{endpoint}", base.trim_end_matches('/')); // 仅拼接公开 /api/v3 端点，不经过 Shell 或账户交易接口。
    let response = client // 公开端点原始响应，HTTP 失败或非法 JSON 不做离线回退。
        .get(url)
        .query(query)
        .send()
        .with_context(|| format!("读取公开 API {endpoint} 失败；请检查网络或代理设置"))?;
    ensure!(
        response.status().is_success(),
        "公开 API {endpoint} 返回 HTTP {}；请检查网络/访问区域/限流，不会自动使用旧规则",
        response.status()
    );
    response
        .json()
        .with_context(|| format!("公开 API {endpoint} 返回的 JSON 无法解析"))
}

/// 输入：单个 JSON 对象与字段名；返回：有限非负的十进制数。
fn decimal_field(value: &Value, name: &str) -> Result<f64> {
    let number: f64 = value[name] // 交易所字符串数字的解析值；拒绝 NaN、无穷和异常巨大值。
        .as_str()
        .with_context(|| format!("缺少十进制字段 {name}"))?
        .parse()
        .with_context(|| format!("字段 {name} 不是有效数字"))?;
    ensure!(
        number.is_finite() && (0.0..=1e12).contains(&number),
        "API 字段 {name} 超出有效范围"
    );
    Ok(number)
}

/// 输入：过滤器列表和类型；返回：相应过滤器，缺失时直接报错。
fn filter<'a>(filters: &'a [Value], kind: &str) -> Result<&'a Value> {
    filters
        .iter()
        .find(|value| value["filterType"] == kind)
        .with_context(|| format!("缺少必需交易过滤器 {kind}"))
}

/// 输入：交易对信息与目标名称；返回：校验后的交易所价格、数量和金额过滤器。
fn read_rules(info: &Value, symbol: &str) -> Result<Rules> {
    let symbols = info["symbols"] // exchangeInfo 交易对数组，不假定目标必定是第一项。
        .as_array()
        .context("exchangeInfo 缺少 symbols")?;
    let row = symbols // 与用户目标名称一致的交易对记录。
        .iter()
        .find(|row| row["symbol"] == symbol)
        .context("exchangeInfo 找不到交易对")?;
    ensure!(
        row["status"] == "TRADING"
            && row["quoteAsset"] == "USDT"
            && row["isSpotTradingAllowed"] == true,
        "交易对必须允许 USDT 现货交易且状态为 TRADING"
    );
    let filters = row["filters"] // 该交易对的全部过滤器，按 filterType 查找。
        .as_array()
        .context("exchangeInfo 缺少 filters")?;
    // 读取/组装的规则快照；必需过滤器缺失就报错。
    let price = filter(filters, "PRICE_FILTER")?; // PRICE_FILTER 对象，包含 tickSize 及价格上下界。
    let qty = filter(filters, "LOT_SIZE")?; // LOT_SIZE 对象，数量限制按基础币而不是 USDT 计。
    let mut rules = Rules {
        tick_size: price["tickSize"].as_str().context("缺少 tickSize")?.into(),
        step_size: qty["stepSize"].as_str().context("缺少 stepSize")?.into(),
        min_notional: 0.0,
        min_qty: decimal_field(qty, "minQty")?,
        max_qty: enabled(decimal_field(qty, "maxQty")?),
        min_price: enabled(decimal_field(price, "minPrice")?),
        max_price: enabled(decimal_field(price, "maxPrice")?),
        max_notional: None,
        max_orders: None,
    };
    read_notional(filters, &mut rules)?;
    if let Some(limit) = filters.iter().find(|v| v["filterType"] == "MAX_NUM_ORDERS") {
        // limit 为交易所订单上限过滤器，不是可用资金。
        rules.max_orders = Some(
            limit["maxNumOrders"]
                .as_u64()
                .context("无效 maxNumOrders")?
                .try_into()?,
        );
    }
    Ok(rules)
}

/// 输入：非负限制值；返回：零表示该限制未启用，否则返回限制值。
fn enabled(number: f64) -> Option<f64> {
    (number > 0.0).then_some(number)
}

/// 输入：过滤器列表和待填规则；返回：无；同时存在两种金额过滤器时取更严格边界。
fn read_notional(filters: &[Value], rules: &mut Rules) -> Result<()> {
    let mut found = false; // 是否遇到有效金额过滤器；缺失时不能假装使用默认最低金额。
    for value in filters {
        // 逐个检查金额规则，多个最低金额限制取较大值。
        if value["filterType"] == "MIN_NOTIONAL" || value["filterType"] == "NOTIONAL" {
            found = true;
            rules.min_notional = rules.min_notional.max(decimal_field(value, "minNotional")?);
            if value["filterType"] == "NOTIONAL" {
                rules.max_notional = enabled(decimal_field(value, "maxNotional")?);
            }
        }
    }
    ensure!(
        found && rules.min_notional > 0.0,
        "缺少有效 MIN_NOTIONAL / NOTIONAL 最低金额过滤器"
    );
    Ok(())
}

/// 输入：原始 Binance K 线、服务器时间及周期；返回：已收盘且时间连续的合法 K 线。
fn closed_klines(value: &Value, now: u64, interval: &str) -> Result<Vec<Candle>> {
    let rows = value.as_array().context("klines 必须为数组")?; // 原始 K 线数组，逐行校验后才用于 ATR。
    let step = interval_ms(interval)?; // 当前 K 线固定周期的毫秒数，用于确定收盘与连续性。
    let mut candles = Vec::new(); // 本次已校验的 OHLC 样本，后续 ATR 与历史展示复用同一批数据。
    for row in rows {
        // row 是一根原始 API K 线，任何格式异常使整批请求失败。
        let fields = row.as_array().context("K 线行必须为数组")?; // Binance K 线行；索引 0 开盘时间、1..4 OHLC、6 收盘时间。
        ensure!(fields.len() >= 7, "K 线行缺少价格或收盘时间字段");
        let open_time = fields[0].as_u64().context("无效 K 线开盘时间")?; // 本行 Unix 开盘毫秒，须与收盘时间匹配所选周期。
        let close_time = fields[6].as_u64().context("无效 K 线收盘时间")?; // 本行 Unix 收盘毫秒，须严格小于服务器时间才保留。
        ensure!(
            open_time.checked_add(step - 1) == Some(close_time),
            "K 线开盘/收盘时间与 interval 不匹配"
        );
        let prices: Vec<f64> = fields[1..5] // 本行开、高、低、收四个字符串价格的数值解析结果。
            .iter()
            .map(|v| {
                v.as_str()
                    .context("K 线价格须为字符串")?
                    .parse()
                    .context("K 线价格无效")
            })
            .collect::<Result<_>>()?;
        // 本行 OHLC 值对象，先检查价格关系再筛选已收盘样本。
        let candle = Candle {
            open_time,
            open: prices[0],
            high: prices[1],
            low: prices[2],
            close: prices[3],
        };
        candle.validate()?;
        if close_time < now {
            candles.push(candle);
        }
    }
    validate_series(&candles, interval)?;
    Ok(candles)
}

/// 输入：行情、已校验 K 线、设置与来源说明；返回：无；附加 Wilder ATR 和数据元信息。
fn attach_atr(
    market: &mut Market,
    candles: &[Candle],
    settings: &Settings,
    source: &str,
) -> Result<()> {
    market.atr = Some(wilder_atr(candles, settings.atr_period)?);
    market.closed_candle_count = candles.len();
    market.last_candle_open_ms = candles.last().map(|c| c.open_time);
    market.atr_source = Some(source.into());
    Ok(())
}
