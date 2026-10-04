// 主线程可观察状态与后台请求协调。

import AppKit
import Combine
import UniformTypeIdentifiers

/// SwiftUI 主线程状态模型，负责请求启动、取消、过时结果隔离与用户导出操作。
/// 阻塞的 HTTP/Rust 进程等待放在 detached Task；所有可观察 UI 状态只在 MainActor 更新。
@MainActor
final class PlannerModel: ObservableObject {
    /// 当前表单；修改时取消旧请求、清理过时方案并保存草稿；行情设置变动也清理历史。
    @Published var form = FormState.load() {
        didSet {
            if running { cancel() }
            if oldValue.historyIdentity != form.historyIdentity { history = nil } // 资金修改无需丢弃历史，来源/周期修改必须丢弃。
            form.save(); result = nil; error = nil; notice = nil
        }
    }
    /// 最近成功生成的方案；输入修改或新请求启动后清空。
    @Published private(set) var result: GeneratedPlan?
    /// 最近成功获取的真实历史快照，供图表与历史导出。
    @Published private(set) var history: GeneratedHistory?
    /// 当前中文错误消息；与成功结果分开显示。
    @Published private(set) var error: String?
    /// 复制、导出、取消等用户操作的简短完成提示。
    @Published private(set) var notice: String?
    /// 是否正在执行请求，用于阻止重复启动并禁用表单。
    @Published private(set) var running = false
    /// 当前请求类型的进度说明，不伪造百分比进度。
    @Published private(set) var activity = ""
    /// 每次请求的唯一令牌；取消/重启后令牌改变，迟到响应不再写入 UI。
    private var generation = UUID()
    /// 当前 Rust 子进程执行器，取消时同时终止子进程。
    private var runner: EngineRunner? // 本次请求专属执行器，Task 与取消动作共享同一实例。
    /// 主线程等待后台结果的任务句柄，完成或取消后清空。
    private var task: Task<Void, Never>?

    /// 输入：当前表单；返回：无；后台执行 Rust，主线程只更新 UI 状态。
    func generate() {
        guard !running else { return }
        result = nil; error = nil; notice = nil
        do { try start(arguments: form.arguments(), historyOnly: false) }
        catch { self.error = error.localizedDescription }
    }

    /// 输入：当前历史周期与代理；返回：无；无需等待资金填写完整即可请求真实历史数据。
    func fetchHistory() {
        guard !running else { return }
        result = nil; history = nil; error = nil; notice = nil
        do { try start(arguments: form.historyArguments(), historyOnly: true) }
        catch { self.error = error.localizedDescription }
    }

    /// 输入：真实命令参数和任务类型；返回：无；共用后台执行、取消及过时响应保护。
    private func start(arguments: [String], historyOnly: Bool) throws {
        guard let resources = Bundle.main.resourceURL else { throw AppFailure(message: "应用资源缺失。") } // resources 是包内资源目录。
        let runner = EngineRunner(executable: resources.appendingPathComponent("grid-planner")) // 本次请求专属执行器，Task 与取消动作共享同一实例。
        self.runner = runner
        running = true
        history = nil
        activity = historyOnly ? "正在获取历史行情…" : "正在生成网格方案…"
        generation = UUID()
        let token = generation // 启动时捕获的请求令牌，用于拒绝取消或新请求之后的迟到响应。
        task = Task { [weak self] in
            do {
                let value = try await Task.detached(priority: .userInitiated) { // 后台阻塞执行返回的不可变结果，主线程校验令牌后才展示。
                    if historyOnly { return EngineResult.history(try runner.fetchHistory(arguments: arguments)) }
                    return EngineResult.plan(try runner.execute(arguments: arguments))
                }.value
                guard let self, token == self.generation, !Task.isCancelled else { return } // 页面仍存在且请求未过时才更新。
                switch value {
                case .plan(let plan): self.result = plan; self.history = try plan.attachedHistory() // 网格结果与同批历史一起展示，避免混用两次请求。
                case .history(let history): self.history = history // 独立获取仅更新历史，不伪造网格方案。
                }
                self.finish()
            } catch {
                guard let self, token == self.generation, !Task.isCancelled else { return } // 页面仍存在且请求未过时才更新。
                self.error = error.localizedDescription
                self.finish()
            }
        }
    }

    /// 输入：无；返回：无；清理一次请求的执行状态。
    private func finish() { running = false; runner = nil; task = nil }

    /// 输入：无；返回：无；使迟到响应失效并终止子进程。
    func cancel() {
        generation = UUID()
        task?.cancel()
        runner?.cancel()
        finish()
        notice = "已取消请求。"
    }

    /// 输入：无；返回：无；恢复默认表单，不自动请求行情。
    func reset() { guard !running else { return }; form = FormState() }

    /// 输入：当前结果；返回：无；只在用户点击复制时修改剪贴板。
    func copy() {
        guard let result, result.plan.isActionable else { return } // 观望方案不能复制为可执行填写参数。
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(result.plan.copyText(), forType: .string)
        notice = "已复制币安填写参数。"
    }

    /// 输入：当前结果；返回：无；用户确认保存位置后才导出原始 JSON。
    func export() {
        guard let result else { return } // 无成功方案时不操作剪贴板或导出文件。
        exportJSON(result.json, name: "\(result.plan.symbol)-grid.json", message: "已导出方案 JSON。")
    }

    /// 输入：当前真实历史快照；返回：无；导出全部 OHLC、ATR、来源与时间，不保存合成行情。
    func exportHistory() {
        guard let history else { return } // 无真实历史快照时不导出示例数据。
        exportJSON(history.json, name: "\(history.history.symbol)-\(history.history.candleInterval)-history.json",
                   message: "已导出历史行情 JSON。")
    }

    /// 输入：JSON、建议文件名及成功提示；返回：无；用户确认位置后原子写入。
    private func exportJSON(_ data: Data, name: String, message: String) {
        guard let window = NSApp.keyWindow else { return } // 保存面板挂到当前参数窗口。
        let panel = NSSavePanel() // 用户确认目标路径的原生保存面板，仅导出点击时创建。
        panel.allowedContentTypes = [.json]
        panel.nameFieldStringValue = name
        panel.canCreateDirectories = true
        panel.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let url = panel.url else { return } // 用户确认后才写 url 指定的位置。
            do { try data.write(to: url, options: .atomic); self?.notice = message }
            catch { self?.error = "导出失败：\(error.localizedDescription)" }
        }
    }
}
