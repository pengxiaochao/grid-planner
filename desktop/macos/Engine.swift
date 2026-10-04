// 桌面与 Rust 的 JSON/子进程桥接；只读行情，不持有账户交易权限。

import Foundation

/// 可跨任务传递的用户可读错误；LocalizedError 让界面保留后端中文原因。
struct AppFailure: LocalizedError, Sendable {
    /// 向用户展示的错误原因，不附带账户凭据。
    let message: String
    /// 输入：当前错误；返回：供 localizedDescription 使用的中文 message。
    var errorDescription: String? { message }
}

/// Rust Plan JSON 的桌面展示模型，snake_case 由解码器映射为 camelCase。
/// 只解码界面需要的字段；精确价格/数量仍保留字符串，避免重新计算改变结果。
struct PlanData: Decodable, Sendable {
    /// 大写 USDT 现货交易对，例如 BTCUSDT。
    let symbol: String
    /// 计算方案时的参考现价，单位 USDT。
    let referencePrice: Double
    /// 已按交易所步长取整的下限字符串，不在桌面再次四舍五入。
    let lowerPrice: String
    /// 已按交易所步长取整的上限字符串，单位 USDT。
    let upperPrice: String
    /// 止损触发价字符串；触发并不保证按该价成交。
    let stopLoss: String
    /// 机器人停止价字符串；区间上限外通常已售出网格库存。
    let takeProfit: String
    /// 网格段数 N，对应 gridPrices 中 N+1 个价格点。
    let gridCount: Int
    /// 每格统一基础币数量字符串，已满足 stepSize。
    let quantityPerGrid: String
    /// 从下到上排列的精确价格点，复制和明细展示复用同一数组。
    let gridPrices: [String]
    /// 用户设定最多可投入的 USDT 金额。
    let capitalLimitUsdt: Double
    /// 满足风险与交易规则的建议实际投入，单位 USDT。
    let investmentUsdt: Double
    /// 投入上限减实际投入后剩余的 USDT。
    let unallocatedUsdt: Double
    /// 账户资产乘风险百分比得到的情景预算，单位 USDT。
    let riskBudgetUsdt: Double
    /// 完整成交且按 SL 清仓的单边下跌亏损估算，单位 USDT。
    let stopScenarioLossUsdt: Double
    /// 进一步跌穿 SL 的压力情景亏损，可大于账户预算。
    let stressScenarioLossUsdt: Double
    /// 全部格中最低的双边扣费净收益百分比，0.3 代表 0.3%。
    let worstNetGridPct: Double
    /// 计入总投入的手续费预留币购买成本，单位 USDT。
    let feeReserveUsdt: Double
    /// offline_inputs 或 binance_public_api，用于说明输入来源。
    let dataSource: String
    /// 所选周期 Wilder ATR 的 USDT 波幅；方案可为 nil，历史结果必须有值。
    let atr: Double?
    /// 请求或计算时使用的 K 线周期，例如 1d。
    let candleInterval: String
    /// 公开服务器 Unix 毫秒；离线方案可为 nil。
    let marketAsOfMs: UInt64?
    /// 后端提供的计算边界/短历史提醒，不删除后再复制给用户。
    let warnings: [String]
    /// 用于显示的历史快照；方案内为可选同批数据，离线不附加合成行情。
    let history: HistoryData?

    /// 输入：方案；返回：方便抄到币安的中文文本，风险信息与关键参数一起保留。
    func copyText() -> String {
        """
        \(symbol) 现货等比网格
        参考现价：\(referencePrice) USDT
        下限：\(lowerPrice)
        上限：\(upperPrice)
        网格数量：\(gridCount)
        投入金额：\(money(investmentUsdt)) USDT
        止损 SL：\(stopLoss)
        停止价 TP：\(takeProfit)
        停止时卖出全部基础币：开启
        每格数量估算：\(quantityPerGrid)
        止损情景亏损：\(money(stopScenarioLossUsdt)) USDT
        账户风险预算：\(money(riskBudgetUsdt)) USDT
        压力情景亏损：\(money(stressScenarioLossUsdt)) USDT
        参数仅供创建前核对，实际成交可能超过风险预算。
        """
    }
}

/// 一次成功计算的不可变结果，同时持有展示模型和原始 JSON 供导出。
struct GeneratedPlan: Sendable {
    /// 本次 JSON 解码后的方案，仅供展示和复制。
    let plan: PlanData
    /// 导出字节；独立请求保留原始后端 JSON，不按界面字段重新删减。
    let json: Data

    /// 输入：方案中同批获取的历史快照；返回：可展示和独立导出的历史数据，离线方案返回 nil。
    func attachedHistory() throws -> GeneratedHistory? {
        guard let history = plan.history else { return nil } // 离线/无历史方案没有可伪造的图表数据。
        let encoder = JSONEncoder() // 重新编码同批附带历史，snake_case 与独立历史导出格式一致。
        encoder.keyEncodingStrategy = .convertToSnakeCase
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return GeneratedHistory(history: history, json: try encoder.encode(history))
    }
}

/// 图表所用单根已收盘 OHLC；Unix 毫秒用于稳定身份，Date 只用于本地时间展示。
struct HistoricalCandle: Codable, Identifiable, Sendable {
    /// K 线 Unix 开盘毫秒，用于唯一身份及时间轴。
    let openTime: UInt64
    /// 周期内开盘价，单位 USDT。
    let open: Double
    /// 周期内最高价，单位 USDT。
    let high: Double
    /// 周期内最低价，单位 USDT。
    let low: Double
    /// 已收盘样本的收盘价，曲线直接使用此值。
    let close: Double
    /// 输入：该根 K 线；返回：开盘毫秒作为 SwiftUI 稳定身份。
    var id: UInt64 { openTime }
    /// 输入：开盘毫秒；返回：转换成秒的 Date，用于时间轴展示。
    var date: Date { Date(timeIntervalSince1970: Double(openTime) / 1000) }
}

/// 公开历史 JSON 的桌面模型；OHLC、ATR、来源和时间来自同一次后端请求。
struct HistoryData: Codable, Sendable {
    /// 大写 USDT 现货交易对，例如 BTCUSDT。
    let symbol: String
    /// 请求或计算时使用的 K 线周期，例如 1d。
    let candleInterval: String
    /// Wilder ATR 平滑周期，默认 14，由后端返回实际值。
    let atrPeriod: Int
    /// 用户请求根数；API 可能返回更少样本。
    let requestedCandleCount: Int
    /// 实际通过收盘与连续性校验的根数，不用请求数替代。
    let closedCandleCount: Int
    /// 获取快照时的最新公开报价，不等同于末根收盘价。
    let currentPrice: Double
    /// 所选周期 Wilder ATR 的 USDT 波幅；方案可为 nil，历史结果必须有值。
    let atr: Double
    /// 首根已收盘样本 Unix 开盘毫秒。
    let firstCandleOpenMs: UInt64
    /// 末根样本 Unix 收盘毫秒，严格早于获取时服务器时间。
    let lastCandleCloseMs: UInt64
    /// 公开服务器 Unix 毫秒；离线方案可为 nil。
    let marketAsOfMs: UInt64
    /// 实际 K 线公开端点，界面显示以便核对来源。
    let sourceUrl: String
    /// 连续且按时间递增的真实 OHLC，图表和明细直接读取。
    let candles: [HistoricalCandle]
    /// 后端提供的计算边界/短历史提醒，不删除后再复制给用户。
    let warnings: [String]
}

/// 一次历史获取的不可变结果；图表读取 history，文件导出使用 json。
struct GeneratedHistory: Sendable {
    /// 用于显示的历史快照；方案内为可选同批数据，离线不附加合成行情。
    let history: HistoryData
    /// 导出字节；独立请求保留原始后端 JSON，不按界面字段重新删减。
    let json: Data
}

/// 共用后台执行路径的两种返回值，区分网格方案和独立历史获取。
enum EngineResult: Sendable {
    /// 当前参数生成的网格方案及原始 JSON。
    case plan(GeneratedPlan)
    /// 独立获取的真实历史快照及原始 JSON。
    case history(GeneratedHistory)
}

/// 输入：金额；返回：固定两位小数的显示文本，格式不依赖系统的小数点地区设置。
func money(_ value: Double) -> String { String(format: "%.2f", locale: Locale(identifier: "en_US_POSIX"), value) }

/// 输入：Unix 毫秒；返回：用户本地时区的可读时间，行情计算仍使用 UTC K 线。
func timestamp(_ milliseconds: UInt64) -> String {
    Date(timeIntervalSince1970: Double(milliseconds) / 1000).formatted(date: .numeric, time: .shortened)
}

/// 真实 Rust 子进程执行器；每个请求独占一个执行器和临时输出目录。
/// @unchecked Sendable 依赖 NSLock 保护 process/cancelled；其余数据不可变。
/// 参数通过 Process.arguments 传递，输出写临时文件避免管道容量造成阻塞。
final class EngineRunner: @unchecked Sendable {
    /// 包内 Rust 可执行文件 URL，生命周期内保持不变。
    private let executable: URL
    /// 保护可变进程引用与取消标志，防止取消和启动交错。
    private let lock = NSLock()
    /// 当前子进程；退出后清空，仅在 lock 内读写。
    private var process: Process?
    /// 取消后保持 true，阻止尚未开始的子进程再次启动。
    private var cancelled = false

    /// 输入：计算程序路径；返回：持有进程控制权的执行器，所有可变进程状态由锁保护。
    init(executable: URL) { self.executable = executable }

    /// 输入：表单转换的参数；返回：真实 Rust 进程生成的方案与原始 JSON。
    func execute(arguments: [String]) throws -> GeneratedPlan {
        let bytes = try executeJSON(arguments: arguments) // 本次 Rust 标准输出的 JSON 原始字节，不读取旧结果。
        return GeneratedPlan(plan: try decode(PlanData.self, from: bytes), json: bytes)
    }

    /// 输入：历史获取参数；返回：公开 API 的已收盘 OHLC、ATR 和可导出原始 JSON。
    func fetchHistory(arguments: [String]) throws -> GeneratedHistory {
        let bytes = try executeJSON(arguments: arguments) // 本次 Rust 标准输出的 JSON 原始字节，不读取旧结果。
        return GeneratedHistory(history: try decode(HistoryData.self, from: bytes), json: bytes)
    }

    /// 输入：JSON 数据及目标类型；返回：已解码的结果或面向用户的格式错误。
    private func decode<T: Decodable>(_ type: T.Type, from bytes: Data) throws -> T {
        let decoder = JSONDecoder() // 只负责 JSON 字段名映射，不重新执行金融计算。
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let value = try? decoder.decode(type, from: bytes) else { // value 必须是目标结果类型。
            throw AppFailure(message: "后端结果格式异常，请重新构建应用。")
        }
        return value
    }

    /// 输入：不经过 Shell 的命令参数；返回：真实子进程的 JSON，取消和错误不会返回旧结果。
    private func executeJSON(arguments: [String]) throws -> Data {
        guard FileManager.default.isExecutableFile(atPath: executable.path) else {
            throw AppFailure(message: "应用缺少计算组件，请重新构建完整 .app。")
        }
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString) // 每次请求随机且独立的临时目录，结束后 defer 清理。
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let output = try outputFile(in: folder, name: "output.json") // 子进程 stdout 文件句柄，避免大历史数据填满 Pipe。
        let errors = try outputFile(in: folder, name: "errors.txt") // 子进程 stderr 文件句柄，失败时读取中文错误链。
        defer { try? output.close(); try? errors.close() }
        let child = Process() // 只启动包内 Rust 可执行文件，参数数组不经过 Shell。
        child.executableURL = executable
        child.arguments = arguments
        child.standardInput = FileHandle.nullDevice
        child.standardOutput = output
        child.standardError = errors
        try start(child)
        child.waitUntilExit()
        let wasCancelled = lock.withLock { process = nil; return cancelled } // 退出后在锁内清空进程引用并读取取消状态，取消优先于旧结果。
        if wasCancelled { throw CancellationError() }
        let bytes = try Data(contentsOf: folder.appendingPathComponent("output.json")) // 本次 Rust 标准输出的 JSON 原始字节，不读取旧结果。
        if child.terminationStatus != 0 {
            let errorBytes = try Data(contentsOf: folder.appendingPathComponent("errors.txt")) // 非零退出码对应的 stderr 字节，保留真实失败原因。
            throw AppFailure(message: String(data: errorBytes, encoding: .utf8) ?? "计算失败，请检查参数。")
        }
        return bytes
    }

    /// 输入：临时目录与文件名；返回：输出文件句柄，用文件避免 Pipe 缓冲区互相等待。
    private func outputFile(in folder: URL, name: String) throws -> FileHandle {
        let url = folder.appendingPathComponent(name) // 当前请求临时目录内的输出文件 URL。
        guard FileManager.default.createFile(atPath: url.path, contents: nil) else {
            throw AppFailure(message: "无法创建临时输出文件。")
        }
        return try FileHandle(forWritingTo: url)
    }

    /// 输入：待启动子进程；返回：无；启动和取消使用同一把锁，避免取消后再启动。
    private func start(_ child: Process) throws {
        try lock.withLock {
            if cancelled { throw CancellationError() }
            try child.run()
            process = child
        }
    }

    /// 输入：无；返回：无；终止本执行器的当前子进程，不影响其他程序。
    func cancel() {
        lock.withLock {
            cancelled = true
            if let process, process.isRunning { process.terminate() } // 只终止本实例持有的活跃子进程。
        }
    }
}
