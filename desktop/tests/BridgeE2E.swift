// 桌面桥接端到端验收：真实表单 → 包内 Rust 进程 → 本地 HTTP → JSON/展示数据。
// 不启动图形窗口、不保存用户草稿；测试行情仅来自独立模拟服务。

import Foundation

/// 可独立编译运行的验收入口，复用生产 FormState/EngineRunner 并调用已打包的真实后端。
@main
struct BridgeE2E {
    /// 输入：打包后真实 Rust 可执行文件路径；返回：成功时退出 0，失败抛出验收错误。
    static func main() throws {
        guard CommandLine.arguments.count == 3 else { throw AppFailure(message: "需要包内后端路径和模拟 API 地址") }
        let engine = URL(fileURLWithPath: CommandLine.arguments[1]) // 应用包内 Rust 路径，确保验收实际发布的组件。
        let api = CommandLine.arguments[2] // Python 模拟服务的回环地址，仅测试时覆盖正式端点。
        try percent(engine)
        try atr(engine)
        try manual(engine)
        try offlineProxy(engine)
        try invalidNumbers(engine)
        try fixedGrids(engine)
        try cancelled(engine)
        try history(engine, api)
        try liveHistory(engine, api)
        try partialHistory(engine, api)
        try invalidHistory(engine, api)
        try adaptive(engine, api)
        try arithmetic(engine)
        try adaptiveArithmetic(engine, api)
        try oldDraft()
        print("desktop bridge E2E: 15 passed（表单 → 真实子进程 → HTTP/JSON → 显示/复制数据）")
    }

    /// 输入：无；返回：明确的离线示例表单，不保存或读取用户偏好。
    private static func draft() -> FormState {
        var form = FormState() // 用例专属表单草稿，修改不污染用户 UserDefaults。
        form.live = false
        form.range = .percent
        form.values["algorithm"] = "classic"
        form.values[FieldID.price.rawValue] = "84000"
        return form
    }

    /// 输入：表单和真实后端；返回：真实生成的完整结果，不替换计算组件。
    private static func calculate(_ form: FormState, _ engine: URL) throws -> GeneratedPlan {
        try EngineRunner(executable: engine).execute(arguments: form.arguments())
    }

    /// 输入：断言值和错误说明；返回：验收通过确认或可定位错误。
    private static func check(_ condition: Bool, _ message: String) throws {
        guard condition else { throw AppFailure(message: message) }
    }

    /// 输入：真实后端；返回：无；核对 JSON 解析、展示值和可复制文本都来自同一份方案。
    private static func percent(_ engine: URL) throws {
        let result = try calculate(draft(), engine) // 经真实子进程解码的结果，展示与导出都从此读取。
        try check(result.plan.gridCount == 3, "默认格数应为 3")
        try check(result.plan.lowerPrice == "75600.00" && result.plan.stopLoss == "73332.00", "线位与 CLI 不一致")
        try check(money(result.plan.investmentUsdt) == "131.81", "投入不一致")
        try check(result.plan.copyText().contains("停止价 TP：95172.00"), "复制文本缺少正确 TP")
        let json = try JSONSerialization.jsonObject(with: result.json) as! [String: Any] // 导出字节的独立 JSON 解析结果，用于对照界面值。
        try check(json["grid_count"] as? Int == result.plan.gridCount, "导出与显示不一致")
    }

    /// 输入：真实后端；返回：无；验证 ATR 表单调用真实计算。
    private static func atr(_ engine: URL) throws {
        var form = draft() // 用例专属表单草稿，修改不污染用户 UserDefaults。
        form.range = .atr
        form.values[FieldID.atr.rawValue] = "2000"
        let p = try calculate(form, engine).plan // 实际计算方案，不在测试中替换后端或硬造结果。
        try check(p.atr == 2000 && p.lowerPrice == "78000.00" && p.stopLoss == "75000.00", "ATR 表单结果异常")
    }

    /// 输入：真实后端；返回：无；旧 ATR 数值不会泄漏进手动区间参数。
    private static func manual(_ engine: URL) throws {
        var form = draft() // 用例专属表单草稿，修改不污染用户 UserDefaults。
        form.range = .manual
        form.values[FieldID.atr.rawValue] = "2000"
        for (field, value) in [(FieldID.lower, "76000"), (.upper, "90000"), (.stop, "73800"), (.take, "92500")] { // field/value 为该用例显式设定的手动线位。
            form.values[field.rawValue] = value
        }
        let p = try calculate(form, engine).plan // 实际计算方案，不在测试中替换后端或硬造结果。
        try check(p.lowerPrice == "76000.00" && p.takeProfit == "92500.00" && p.atr == nil, "模式切换带入了旧参数")
    }

    /// 输入：真实后端；返回：无；离线不会访问手填代理或被无效代理阻断。
    private static func offlineProxy(_ engine: URL) throws {
        var form = draft() // 用例专属表单草稿，修改不污染用户 UserDefaults。
        form.useProxy = true
        form.values[FieldID.proxyPort.rawValue] = "invalid"
        let p = try calculate(form, engine).plan // 实际计算方案，不在测试中替换后端或硬造结果。
        try check(p.gridCount == 3 && p.dataSource == "offline_inputs", "离线错误使用了代理")
    }

    /// 输入：真实后端；返回：无；前端完整链路捕获计算进程的非法数字错误。
    private static func invalidNumbers(_ engine: URL) throws {
        var form = draft() // 用例专属表单草稿，修改不污染用户 UserDefaults。
        form.values[FieldID.capital.rawValue] = "NaN"
        do {
            _ = try calculate(form, engine)
            throw AppFailure(message: "不应返回非法资金的成功结果")
        } catch let error as AppFailure {
            try check(error.message.contains("有限数"), "非法数字错误未正确传播：\(error.message)")
        }
    }

    /// 输入：真实后端；返回：无；不满足约束的固定格数不会被偷偷替换。
    private static func fixedGrids(_ engine: URL) throws {
        var form = draft() // 用例专属表单草稿，修改不污染用户 UserDefaults。
        form.values[FieldID.grids.rawValue] = "150"
        do {
            _ = try calculate(form, engine)
            throw AppFailure(message: "不可行固定格数不应成功")
        } catch let error as AppFailure {
            try check(error.message.contains("无可行网格方案"), "固定格数错误未传播")
        }
    }

    /// 输入：真实后端；返回：无；取消已经发生时不会再启动后端或返回过时结果。
    private static func cancelled(_ engine: URL) throws {
        let runner = EngineRunner(executable: engine) // 请求专属执行器，先取消再执行以验证启动竞争保护。
        runner.cancel()
        do {
            _ = try runner.execute(arguments: draft().arguments())
            throw AppFailure(message: "取消后不应再生成结果")
        } catch is CancellationError { }
    }

    /// 输入：无；返回：历史请求草稿，本金、现价和手动区间均为空。
    private static func historyDraft() -> FormState {
        var form = FormState() // 用例专属表单草稿，修改不污染用户 UserDefaults。
        form.range = .manual
        form.values[FieldID.capital.rawValue] = ""
        form.values[FieldID.historyBars.rawValue] = "30"
        return form
    }

    /// 输入：表单、包内后端和模拟 API；返回：完整 HTTP + 子进程链路的历史快照。
    private static func download(_ form: FormState, _ engine: URL, _ api: String) throws -> GeneratedHistory {
        try EngineRunner(executable: engine).fetchHistory(arguments: form.historyArguments() + ["--api-base-url", api])
    }

    /// 输入：真实后端及模拟 API；返回：无；独立获取历史并核对用于图表、ATR 和导出的数据。
    private static func history(_ engine: URL, _ api: String) throws {
        let result = try download(historyDraft(), engine, api) // 经真实子进程解码的结果，展示与导出都从此读取。
        let h = result.history // 实际返回的历史数据，检查样本/ATR/图表字段及短历史提醒。
        try check(h.symbol == "BTCUSDT" && h.closedCandleCount == 30, "历史表单根数未生效")
        try check(h.atr == 2000 && h.candles.count == 30, "历史 ATR / 曲线数据不一致")
        try check(h.candles.allSatisfy { $0.high == 85000 }, "历史包含未收盘 K 线")
        try check(h.lastCandleCloseMs < h.marketAsOfMs && h.sourceUrl.hasPrefix(api), "历史来源或时间缺失")
        let json = try JSONSerialization.jsonObject(with: result.json) as! [String: Any] // 导出字节的独立 JSON 解析结果，用于对照界面值。
        try check(json["closed_candle_count"] as? Int == h.closedCandleCount, "历史导出与展示不一致")
    }

    /// 输入：真实后端及模拟 API；返回：无；联网生成使用同一批历史 K 线而不是本地数据。
    private static func liveHistory(_ engine: URL, _ api: String) throws {
        var form = FormState() // 用例专属表单草稿，修改不污染用户 UserDefaults。
        form.values[FieldID.historyBars.rawValue] = "30"
        form.values["algorithm"] = "classic"
        let result = try EngineRunner(executable: engine).execute(arguments: form.arguments() + ["--api-base-url", api]) // 经真实子进程解码的结果，展示与导出都从此读取。
        try check(result.plan.history?.closedCandleCount == 30, "方案没有所选历史数据")
        try check(result.plan.atr == result.plan.history?.atr && result.plan.lowerPrice == "78000.00", "线位未使用真实历史 ATR")
    }

    /// 输入：真实后端及模拟 API；返回：无；短历史保留实际根数并向界面提供提醒。
    private static func partialHistory(_ engine: URL, _ api: String) throws {
        var form = historyDraft() // 用例专属表单草稿，修改不污染用户 UserDefaults。
        form.values[FieldID.historyBars.rawValue] = "180"
        let h = try download(form, engine, api).history // 实际返回的历史数据，检查样本/ATR/图表字段及短历史提醒。
        try check(h.requestedCandleCount == 180 && h.closedCandleCount == 100, "短历史根数报告不真实")
        try check(!h.warnings.isEmpty, "短历史没有可见提醒")
    }

    /// 输入：真实后端及模拟 API；返回：无；不足 ATR 周期的历史设置在请求前拒绝。
    private static func invalidHistory(_ engine: URL, _ api: String) throws {
        var form = historyDraft() // 用例专属表单草稿，修改不污染用户 UserDefaults。
        form.values[FieldID.historyBars.rawValue] = "14"
        do {
            _ = try download(form, engine, api)
            throw AppFailure(message: "不足样本的历史设置不应成功")
        } catch let error as AppFailure {
            try check(error.message.contains("history-bars"), "历史参数错误未传播")
        }
    }

    /// 输入：包内后端和测试 API；返回：无；自适应参数、指标解码、导出和观望复制提示一致。
    private static func adaptive(_ engine: URL, _ api: String) throws {
        var form = FormState()
        form.values["algorithm"] = "adaptive"
        form.values[FieldID.capital.rawValue] = "3000"
        form.values[FieldID.equity.rawValue] = "15000"
        form.values[FieldID.maxGrids.rawValue] = "24"
        form.values[FieldID.minOrder.rawValue] = "10"
        let result = try EngineRunner(executable: engine).execute(arguments: form.arguments() + ["--api-base-url", api + "/adaptive"])
        let report = result.plan.optimization
        try check(result.plan.algorithm == "adaptive" && report != nil, "自适应 JSON 未解码")
        try check(report?.holdout.evaluatedBars == 36, "最终检验没有保留 20% 历史")
        try check(result.plan.copyText().contains("最终检验"), "复制文本缺少历史证据")
        let json = try JSONSerialization.jsonObject(with: result.json) as! [String: Any]
        try check(json["optimization"] != nil, "导出丢失选参审计数据")
        form.values[FieldID.historyBars.rawValue] = "60"
        do {
            _ = try EngineRunner(executable: engine).execute(arguments: form.arguments() + ["--api-base-url", api + "/adaptive"])
            throw AppFailure(message: "短历史不能生成自适应方案")
        } catch let error as AppFailure {
            try check(error.message.contains("自适应"), "短历史错误没有传播")
        }
    }

    /// 输入：包内后端；返回：无；等差选择经表单传参，显示/复制和导出的价位及类型一致。
    private static func arithmetic(_ engine: URL) throws {
        var form = draft()
        form.range = .manual
        for (field, value) in [("grid-mode", "arithmetic"), ("lower", "400"), ("upper", "450"),
                               ("price", "425"), ("equity", "30000"), ("grids", "5"), ("step-size", "0.001")] {
            form.values[field] = value
        }
        let result = try calculate(form, engine)
        let json = try JSONSerialization.jsonObject(with: result.json) as! [String: Any]
        try check(json["mode"] as? String == "arithmetic", "表单等差类型未传入后端")
        try check(result.plan.gridPrices == ["400.00", "410.00", "420.00", "430.00", "440.00", "450.00"], "等差价位不正确")
        try check(result.plan.copyText().contains("现货等差网格"), "复制文本未保留等差类型")
        try check((json["grid_count_reason"] as? String)?.contains("用户指定") == true, "缺少固定格数原因")
    }

    /// 输入：包内后端和测试 API；返回：无；真实历史自适应等差仍保留成本审计并支持超过五格。
    private static func adaptiveArithmetic(_ engine: URL, _ api: String) throws {
        var form = FormState()
        for (field, value) in [("grid-mode", "arithmetic"), ("capital", "3000"), ("equity", "15000"),
                               ("max-grids", "24"), ("min-order-usdt", "10"), ("grids", "6")] {
            form.values[field] = value
        }
        let result = try EngineRunner(executable: engine).execute(arguments: form.arguments() + ["--api-base-url", api + "/adaptive"])
        let json = try JSONSerialization.jsonObject(with: result.json) as! [String: Any]
        try check(json["mode"] as? String == "arithmetic", "自适应使用了错误网格类型")
        try check(result.plan.gridCount == 6 && result.plan.optimization?.holdout.evaluatedBars == 36, "固定格数或最终检验异常")
        try check(result.plan.copyText().contains("等差") || !result.plan.isActionable, "可执行复制缺少等差类型")
    }

    /// 输入：无；返回：无；旧版表单 JSON 缺少算法字段时补默认值，不丢其他设置。
    private static func oldDraft() throws {
        let data = Data("{\"live\":true,\"range\":\"atr\",\"useProxy\":false,\"proxy\":\"http\",\"values\":{\"capital\":\"321\"}}".utf8)
        let form = try JSONDecoder().decode(FormState.self, from: data)
        try check(form.value(.capital) == "321", "升级丢失旧资金设置")
        try check(form.arguments().contains("adaptive"), "旧草稿没有自适应默认值")
        try check(form.arguments().contains("--grid-mode") && form.arguments().contains("geometric"), "旧草稿没有补等比默认值")
    }
}
