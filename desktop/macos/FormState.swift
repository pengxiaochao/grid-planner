// 表单规格、模式参数组装、代理校验及本地草稿持久化。

import Foundation

/// 桌面区间模型选择，rawValue 与 Rust CLI 的 --mode 一致并用于持久化。
enum RangeChoice: String, Codable, CaseIterable, Sendable {
    /// 按现价百分比展开，百分比为用户假设。
    case percent
    /// 按已收盘 ATR 倍数展开，只反映历史波幅。
    case atr
    /// 用户指定箱体边界，后端仍校验资金与精度。
    case manual
    /// 输入：当前区间模式；返回：百分比、ATR 波动或手动区间的中文标签。
    var title: String {
        switch self { case .percent: "百分比"; case .atr: "ATR 波动"; case .manual: "手动区间" }
    }
}

/// 可选择的代理协议；socks5h 让代理解析目标域名，适用于本地 DNS 不通的情况。
enum ProxyChoice: String, Codable, CaseIterable, Sendable {
    /// HTTP 代理；HTTPS 目标由 HTTP CONNECT 建立隧道。
    case http
    /// SOCKS5 代理，并由代理端解析目标域名。
    case socks5h
    /// 输入：当前代理协议；返回：HTTP 或带远程 DNS 说明的 SOCKS5 标签。
    var title: String { self == .http ? "HTTP" : "SOCKS5（远程 DNS）" }
}

/// 所有表单字段的稳定标识；rawValue 通常就是 Rust CLI 参数名。
/// proxyHost/proxyPort 只用于界面，最终组合成一个 --proxy-url 参数。
enum FieldID: String, CaseIterable, Sendable {
    /// 选参算法 classic/adaptive，旧草稿缺失时使用自适应默认值。
    case algorithm
    /// 本次允许投入的上限，单位 USDT；不是必须全部投入的金额。
    case capital
    /// 账户总资产，单位 USDT；留空时以 capital 作为风险预算基数。
    case equity
    /// 大写 USDT 现货交易对，例如 BTCUSDT；不支持合约或其他报价币。
    case symbol
    /// 离线参考现价，单位 USDT/基础币；联网时使用公开报价。
    case price
    /// 手填的 ATR 绝对价格波幅，单位 USDT；联网时由公开 K 线计算。
    case atr
    /// 手动模式的区间下限，单位 USDT；其他模式不接受此字段。
    case lower
    /// 手动模式的区间上限，单位 USDT；现价必须位于上下限之间。
    case upper
    /// 固定 K 线周期字符串，例如 1d；同时决定连续性及收盘边界。
    case interval
    /// 本策略止损情景占账户总资产的预算百分比；不保证实际亏损上限。
    case risk = "risk-pct"
    /// 百分比模式向下幅度，填 10 表示 10%，不是 0.10。
    case down = "down-pct"
    /// 百分比模式向上幅度，填 10 表示 10%。
    case up = "up-pct"
    /// 非 ATR 模式的区间外止损/停止价缓冲，填 3 表示 3%。
    case outside = "outside-pct"
    /// 买入、卖出各自的手续费百分比；两边分别计算，不重复扣双边费率。
    case fee = "fee-pct"
    /// 每边额外成本/滑点假设百分比，与 fee_pct 相加后用于情景计算。
    case slippage = "slippage-pct"
    /// 用户要求的最低单笔金额，单位 USDT；与交易所最低金额取较大值。
    case minOrder = "min-order-usdt"
    /// 价格取整后最差一格扣除双边成本的收益门槛，单位百分比。
    case minNet = "min-net-pct"
    /// 投入中预留手续费基础币成本的百分比；预留部分也计入下跌风险。
    case reserve = "reserve-pct"
    /// 压力情景中，实际卖出价低于止损价的额外跌幅百分比。
    case stress = "stress-pct"
    /// 自动搜索的最多网格段数；还受交易所最大订单数限制。
    case maxGrids = "max-grids"
    /// 可选固定网格段数；留空则自动搜索，不可行的固定值会报错而非改值。
    case grids
    /// 可选止损覆盖价，单位 USDT；留空则自动推导，显式值必须低于下限。
    case stop = "stop-loss"
    /// 可选机器人停止价，单位 USDT；留空则自动推导，显式值必须高于上限。
    case take = "take-profit"
    /// Wilder ATR 平滑周期；计算首个真实波幅还需要一根前置 K 线。
    case atrPeriod = "atr-period"
    /// ATR 模式下，从现价向上下各展开多少倍 ATR。
    case rangeAtr = "range-atr-mult"
    /// 可选历史请求根数，最多 1000；必须大于 atr_period 才能计算 ATR。
    case historyBars = "history-bars"
    /// ATR 模式下，从区间下限再向下缓冲多少倍 ATR。
    case stopAtr = "stop-atr-mult"
    /// ATR 模式下，从区间上限再向上缓冲多少倍 ATR。
    case takeAtr = "take-atr-mult"
    /// 离线价格步长字符串，例如 0.01；保留十进制位数供精确输出。
    case tick = "tick-size"
    /// 离线基础币数量步长字符串，例如 0.00001。
    case step = "step-size"
    /// 离线交易所单笔最低名义金额，单位 USDT；联网由实际过滤器覆盖。
    case minNotional = "min-notional"
    /// 离线交易所单笔最低基础币数量；不是 USDT 金额。
    case minQty = "min-qty"
    /// 离线交易所单笔最高基础币数量；留空表示不添加此上限。
    case maxQty = "max-qty"
    /// 离线交易所单笔最高名义金额，单位 USDT；留空表示未启用。
    case maxNotional = "max-notional"
    /// 代理 IP/主机文本，IPv6 在组装 URL 时加方括号；不含协议或路径。
    case proxyHost = "proxy-host"
    /// 代理端口文本，须能解析为 1...65535 的整数。
    case proxyPort = "proxy-port"

    /// 输入：字段标识；返回：完整 catalog 中对应的展示规格；新增字段须同步登记。
    var spec: FieldSpec { FieldSpec.catalog[self]! }
}

/// 一个输入框的展示名称、初始文本及占位提示，集中维护以避免视图重复定义。
struct FieldSpec: Sendable {
    /// 用户可见的字段名称；金额/数量/百分比的适用单位在标题中标明。
    let title: String
    /// 新表单初始文本；空值表示必填未填或选填沿用后端默认。
    let initial: String
    /// 空输入框的说明，不作为实际数值传给 Rust。
    let placeholder: String
    /// 全部字段的默认展示规格；费率、缓冲和倍数为可调假设，价格不填旧行情。
    static let catalog: [FieldID: FieldSpec] = [
        .algorithm: .init(title: "选参算法", initial: "adaptive", placeholder: "adaptive"),
        .capital: .init(title: "最多投入 · USDT", initial: "600", placeholder: "600"),
        .equity: .init(title: "账户资产 · USDT", initial: "", placeholder: "留空 = 最多投入"),
        .symbol: .init(title: "交易对", initial: "BTCUSDT", placeholder: "BTCUSDT"),
        .price: .init(title: "当前价格 · USDT", initial: "", placeholder: "填写币安当前价格"),
        .risk: .init(title: "账户风险 · %", initial: "2", placeholder: "2 表示 2%"),
        .atr: .init(title: "ATR 价格波幅", initial: "", placeholder: "图表 ATR 数值，如 2000"),
        .lower: .init(title: "区间下限 · USDT", initial: "", placeholder: "你确定的下限"),
        .upper: .init(title: "区间上限 · USDT", initial: "", placeholder: "你确定的上限"),
        .interval: .init(title: "K 线周期", initial: "1d", placeholder: "1d"),
        .historyBars: .init(title: "历史 K 线根数", initial: "180", placeholder: "大于 ATR 周期，最多 1000"),
        .down: .init(title: "向下幅度 · %", initial: "10", placeholder: "10"),
        .up: .init(title: "向上幅度 · %", initial: "10", placeholder: "10"),
        .outside: .init(title: "区间外缓冲 · %", initial: "3", placeholder: "3"),
        .fee: .init(title: "每边手续费 · %", initial: "0.1", placeholder: "0.1"),
        .slippage: .init(title: "每边成本余量 · %", initial: "0.05", placeholder: "0.05"),
        .minOrder: .init(title: "最低单笔 · USDT", initial: "30", placeholder: "30"),
        .minNet: .init(title: "单格净收益门槛 · %", initial: "0.3", placeholder: "0.3"),
        .reserve: .init(title: "手续费预留 · %", initial: "5", placeholder: "5"),
        .stress: .init(title: "跌穿止损压力 · %", initial: "5", placeholder: "5"),
        .maxGrids: .init(title: "格数搜索上限", initial: "150", placeholder: "2～170"),
        .grids: .init(title: "固定格数（选填）", initial: "", placeholder: "留空自动计算"),
        .stop: .init(title: "止损 SL（选填）", initial: "", placeholder: "留空自动计算"),
        .take: .init(title: "停止价 TP（选填）", initial: "", placeholder: "留空自动计算"),
        .atrPeriod: .init(title: "ATR 周期", initial: "14", placeholder: "14"),
        .rangeAtr: .init(title: "区间 ATR 倍数", initial: "3", placeholder: "3"),
        .stopAtr: .init(title: "止损 ATR 倍数", initial: "1.5", placeholder: "1.5"),
        .takeAtr: .init(title: "停止价 ATR 倍数", initial: "1.5", placeholder: "1.5"),
        .tick: .init(title: "价格步长", initial: "0.01", placeholder: "交易所 tickSize"),
        .step: .init(title: "数量步长", initial: "0.00001", placeholder: "交易所 stepSize"),
        .minNotional: .init(title: "交易所最低 · USDT", initial: "10", placeholder: "minNotional"),
        .minQty: .init(title: "交易所最低数量", initial: "0.00001", placeholder: "minQty"),
        .maxQty: .init(title: "交易所最高数量", initial: "", placeholder: "选填"),
        .maxNotional: .init(title: "交易所最高 · USDT", initial: "", placeholder: "选填"),
        .proxyHost: .init(title: "代理地址 / IP", initial: "127.0.0.1", placeholder: "127.0.0.1 或代理主机"),
        .proxyPort: .init(title: "代理端口", initial: "7890", placeholder: "填写代理软件端口")
    ]
}

/// 可保存/恢复的表单草稿；数值先保留文本，再交给 Rust 统一校验。
/// 区间切换仅传当前模型需要的字段，避免残留 ATR 或手动边界污染新请求。
struct FormState: Codable, Equatable, Sendable {
    /// 是否使用真实公开行情；默认开启，但应用启动不会自动请求。
    var live = true
    /// 当前区间模型；默认 ATR，须在生成时获取已收盘样本。
    var range = RangeChoice.atr
    /// 是否显式使用用户代理；只影响本应用公开 HTTP 请求。
    var useProxy = false
    /// 选定代理协议，默认 HTTP；SOCKS5h 使用远程 DNS。
    var proxy = ProxyChoice.http
    /// 以稳定 CLI 字段名为键的文本草稿；从 catalog 填初值，支持 Codable 持久化。
    var values = Dictionary(uniqueKeysWithValues: FieldID.allCases.map { ($0.rawValue, $0.spec.initial) })

    /// 输入：字段标识；返回：清理首尾空白后的输入文本。
    func value(_ field: FieldID) -> String {
        (values[field.rawValue] ?? field.spec.initial).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// 输入：当前表单；返回：用于 Process.arguments 的参数数组，完全不经过 Shell。
    func arguments() throws -> [String] {
        try require([.capital, .symbol])
        var args = ["--json", "--mode", range.rawValue, "--algorithm", value(.algorithm)] // 算法选择显式传入，不因历史不足悄悄切回旧版。
        append([.capital, .equity, .risk, .fee, .slippage, .minOrder, .minNet,
                .reserve, .stress, .maxGrids, .grids, .stop, .take], to: &args)
        args += ["--symbol", value(.symbol).uppercased()]
        if live { args += ["--live"] + (try historyOptions()) } else {
            try require([.price])
            append([.price, .tick, .step, .minNotional, .minQty, .maxQty, .maxNotional], to: &args)
        }
        switch range {
        case .percent: append([.down, .up, .outside], to: &args)
        case .manual:
            try require([.lower, .upper])
            append([.lower, .upper, .outside], to: &args)
        case .atr:
            append([.rangeAtr, .stopAtr, .takeAtr], to: &args)
            if !live { try require([.atr]); append([.atr, .interval, .atrPeriod], to: &args) }
        }
        return args
    }

    /// 输入：当前行情与代理设置；返回：独立联网获取历史的参数，不要求资金或手动价格。
    func historyArguments() throws -> [String] {
        try require([.symbol])
        return ["--fetch-history", "--json", "--symbol", value(.symbol).uppercased()] + (try historyOptions())
    }

    /// 输入：当前表单；返回：历史快照依赖的设置标识，用于清理交易对或周期变化后的旧数据。
    var historyIdentity: [String] {
        [value(.symbol).uppercased(), value(.interval), value(.historyBars), value(.atrPeriod),
         String(live), String(useProxy), proxy.rawValue, value(.proxyHost), value(.proxyPort)]
    }

    /// 输入：周期、根数和代理；返回：两个联网入口共同使用的选项，由 Rust 校验数值范围。
    private func historyOptions() throws -> [String] {
        try require([.interval, .historyBars, .atrPeriod])
        var args: [String] = [] // 明确的 CLI 参数数组，空白选填项不添加，保留 Rust 默认值。
        append([.interval, .historyBars, .atrPeriod], to: &args)
        if useProxy { args += ["--proxy-url", try proxyURL()] }
        return args
    }

    /// 输入：字段组和参数数组；返回：无；空白选填项沿用 Rust 默认值。
    private func append(_ fields: [FieldID], to args: inout [String]) {
        for field in fields where !value(field).isEmpty { args += ["--\(field.rawValue)", value(field)] } // 当前非空字段变成独立 flag/value 参数，不插值到 Shell。
    }

    /// 输入：必填字段组；返回：完整确认或面向用户的错误。
    private func require(_ fields: [FieldID]) throws {
        for field in fields where value(field).isEmpty { // 发现缺失的必填字段时使用对应中文名称提示。
            throw AppFailure(message: "请填写「\(field.spec.title)」。")
        }
    }

    /// 输入：代理协议、主机及端口；返回：经过基本校验的代理 URL，IPv6 自动加括号。
    private func proxyURL() throws -> String {
        let host = value(.proxyHost) // 清理空白后的代理主机，协议和路径不应混在此字段。
        guard !host.isEmpty, !host.contains(where: { $0.isWhitespace || "/?@#".contains($0) }),
              !host.contains("://"), let port = Int(value(.proxyPort)), (1...65535).contains(port) else { // port 是校验后的整数端口。
            throw AppFailure(message: "代理只填 IP / 主机和 1～65535 的端口，不要在地址栏填协议或路径。")
        }
        let address = host.contains(":") && !host.hasPrefix("[") ? "[\(host)]" : host // IPv6 补上方括号后的主机表示，普通域名/IP 保持原样。
        let text = "\(proxy.rawValue)://\(address):\(port)" // 由当前协议、主机和有效端口组成的代理 URL。
        guard URLComponents(string: text)?.host != nil else { throw AppFailure(message: "代理地址格式错误。") }
        return text
    }

    /// 输入：当前表单；返回：无；仅保存填写的参数，不保存交易结果或自动发起请求。
    func save() {
        if let data = try? JSONEncoder().encode(self) { UserDefaults.standard.set(data, forKey: "grid-form-v1") } // data 只包含表单草稿。
    }

    /// 输入：应用偏好；返回：上次表单，缺失字段使用默认值以支持后续升级。
    static func load() -> FormState {
        guard let data = UserDefaults.standard.data(forKey: "grid-form-v1"), // 读取旧版兼容的表单 JSON。
              var saved = try? JSONDecoder().decode(FormState.self, from: data) else { return FormState() } // 缺失或损坏则用初始草稿。
        for field in FieldID.allCases where saved.values[field.rawValue] == nil { // 旧草稿没有的新字段以当前 catalog 默认值补齐。
            saved.values[field.rawValue] = field.spec.initial
        }
        return saved
    }
}
