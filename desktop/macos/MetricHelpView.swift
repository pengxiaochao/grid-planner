// 窗口内共用的指标说明按钮和 toast；只改变帮助显示状态，不接触表单或行情。

import SwiftUI

/// 一次指标说明的独立身份，用于防止旧定时器关闭新提示。
struct MetricTip: Identifiable {
    /// 每次点击生成的新身份，即使重复点击同一指标也重新计时。
    let id = UUID()
    /// 用户实际点击的指标标题。
    let title: String
    /// 给新手阅读的正文。
    let message: String
}

/// 整个参数窗口共用的帮助状态，与交易状态相互独立。
@MainActor
final class MetricHelpPresenter: ObservableObject {
    /// 当前提示，nil 时不显示覆盖层。
    @Published var tip: MetricTip?
    /// 鼠标在提示上时暂停自动隐藏，离开后重新计时。
    @Published var hovering = false

    /// 输入：界面标题及可选稳定说明键；返回：无；只显示已登记的指标说明。
    func show(_ title: String, key: String? = nil) {
        guard let message = MetricGuide.explanation(for: key ?? title) else { return }
        tip = MetricTip(title: title, message: message)
    }

    /// 输入：可选旧提示身份；返回：无；旧计时任务不能关闭当前新提示。
    func dismiss(id: UUID? = nil) {
        if id == nil || tip?.id == id {
            tip = nil
            hovering = false
        }
    }
}

/// 结果行、输入行及价格卡共用标签；问号可由鼠标或键盘触发。
struct MetricHelpLabel: View {
    /// 用户可见的指标名称，不改动原有数值与单位。
    let title: String
    /// 动态标签或输入字段的稳定说明键，缺省使用标题。
    var key: String? = nil
    /// 整个窗口共用的帮助状态，由根视图注入。
    @EnvironmentObject private var help: MetricHelpPresenter

    /// 输入：标题和说明键；返回：带说明入口的指标标签，未登记时不显示空问号。
    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 4) {
            Text(title)
            if MetricGuide.explanation(for: key ?? title) != nil {
                Button { help.show(title, key: key) } label: {
                    Image(systemName: "questionmark.circle").foregroundStyle(.blue.opacity(0.8))
                }.buttonStyle(.plain)
                    .help("点击解释：\(title)")
                    .accessibilityLabel("解释：\(title)")
                    .accessibilityIdentifier("metric-help.\(key ?? title)")
            }
        }
    }
}

/// 固定在窗口右下角的非模态说明；可复制、关闭，停留阅读时暂停自动隐藏。
struct MetricHelpToast: View {
    /// 根窗口注入的帮助状态，只观察当前提示。
    @ObservedObject var presenter: MetricHelpPresenter

    /// 输入：当前说明与鼠标状态；返回：最多一张说明卡，不阻塞其余界面。
    var body: some View {
        if let tip = presenter.tip {
            VStack(alignment: .leading, spacing: 10) {
                HStack(alignment: .top) {
                    Label(tip.title, systemImage: "info.circle.fill").font(.headline)
                    Spacer(minLength: 12)
                    Button { presenter.dismiss() } label: { Image(systemName: "xmark") }
                        .buttonStyle(.plain).keyboardShortcut(.cancelAction)
                        .accessibilityLabel("关闭指标说明").accessibilityIdentifier("metric-help.close")
                }
                Text(tip.message).font(.callout).lineSpacing(4).fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled).accessibilityIdentifier("metric-help.message")
                Text("12 秒后自动收起 · 鼠标停留可继续阅读 · Esc 关闭")
                    .font(.caption2).foregroundStyle(.secondary)
            }.padding(16).frame(width: 410, alignment: .leading)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
                .overlay(RoundedRectangle(cornerRadius: 14).stroke(.blue.opacity(0.25)))
                .shadow(color: .black.opacity(0.16), radius: 12, y: 4).padding(18)
                .onHover { presenter.hovering = $0 }
                .task(id: "\(tip.id)-\(presenter.hovering)") { await dismissLater(tip.id) }
                .accessibilityElement(children: .contain).accessibilityIdentifier("metric-help.toast")
        }
    }

    /// 输入：当前提示身份；返回：无；鼠标离开后等 12 秒，取消或已换提示时不关闭新卡。
    @MainActor private func dismissLater(_ id: UUID) async {
        guard !presenter.hovering else { return }
        do { try await Task.sleep(for: .seconds(12)) } catch { return }
        presenter.dismiss(id: id)
    }
}
