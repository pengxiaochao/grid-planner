// 原生 SwiftUI 参数界面、结果摘要及真实历史图表。

import AppKit
import Charts
import SwiftUI

/// 参数窗口的整体布局：左侧输入、右侧结果、底部生成/取消/复制/导出操作。
@MainActor
struct PlannerView: View {
    /// 共享的主线程状态；ObservedObject 在表单或结果变化时刷新视图。
    @ObservedObject var model: PlannerModel
    /// 输入：当前共享模型/卡片属性；返回：SwiftUI 主布局，不发起隐式请求。
    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            HStack(spacing: 0) {
                FormPane(model: model).frame(width: 390)
                Divider()
                ResultPane(model: model).frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            Divider()
            actions
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .tint(.blue)
    }

    /// 输入：模型操作状态；返回：标题、说明入口及恢复默认按钮。
    private var header: some View {
        HStack(spacing: 12) {
            Image(systemName: "chart.line.uptrend.xyaxis")
                .font(.system(size: 24, weight: .semibold)).foregroundStyle(.blue)
                .frame(width: 48, height: 48).background(.blue.opacity(0.10), in: RoundedRectangle(cornerRadius: 12))
            VStack(alignment: .leading, spacing: 4) {
                Text("网格交易助手").font(.system(size: 21, weight: .semibold))
                Text("USDT 现货 · 等差 / 等比网格 · 账户风险预算").font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Button("使用说明", action: openHelp)
            Button("恢复默认", action: model.reset).disabled(model.running)
        }.padding(18)
    }

    /// 输入：请求与结果状态；返回：生成、取消、复制及方案导出操作区。
    private var actions: some View {
        HStack(spacing: 10) {
            Button(model.running ? model.activity : "生成网格方案", action: model.generate)
                .buttonStyle(.borderedProminent).keyboardShortcut(.defaultAction).disabled(model.running)
            if model.running {
                ProgressView().controlSize(.small)
                Button("取消", action: model.cancel)
            }
            if let notice = model.notice { Text(notice).font(.caption).foregroundStyle(.secondary) } // notice 是复制/取消等操作的完成消息。
            Spacer()
            Button("复制填写参数", action: model.copy).disabled(model.result?.plan.isActionable != true)
            Button("导出 JSON", action: model.export).disabled(model.result == nil)
        }.padding(16)
    }

    /// 输入：包内 README；返回：无；用户点击后打开本地使用说明。
    private func openHelp() {
        if let url = Bundle.main.url(forResource: "README", withExtension: "md") { NSWorkspace.shared.open(url) } // url 为包内本地说明，不依赖源码目录。
    }
}

/// 复用 FieldID/FieldSpec 的输入面板；只展示当前联网和区间模式需要的字段。
@MainActor
struct FormPane: View {
    /// 共享的主线程状态；ObservedObject 在表单或结果变化时刷新视图。
    @ObservedObject var model: PlannerModel
    /// 输入：当前共享模型/卡片属性；返回：SwiftUI 主布局，不发起隐式请求。
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                GroupBox("资金与行情") { account }
                GroupBox("区间计算方式") { strategy }
                if model.form.live { GroupBox("真实历史行情") { historicalSettings } }
                GroupBox("代理连接") { proxy }
                DisclosureGroup("费用、止损与风险细节") { advanced.padding(.top, 10) }
                if !model.form.live {
                    DisclosureGroup("离线交易规则") {
                        VStack(spacing: 10) {
                            rows([.tick, .step, .minNotional, .minQty, .maxQty, .maxNotional])
                            Text("离线默认规则是示例值，请按交易对实际规则填写。")
                                .font(.caption).foregroundStyle(.secondary)
                        }.padding(.top, 10)
                    }
                }
                Text("填写内容自动保存。启动应用不会自动读取行情或生成方案。")
                    .font(.caption).foregroundStyle(.secondary)
            }.padding(18)
        }.disabled(model.running)
    }

    /// 输入：资金与联网选择；返回：本金、账户风险、交易对与可选手填价格。
    private var account: some View {
        VStack(alignment: .leading, spacing: 11) {
            rows([.symbol, .capital, .equity, .risk])
            Text("最多投入 ≠ 必须全部投入。风险预算不足时会自动缩小金额。")
                .font(.caption).foregroundStyle(.secondary)
            Divider()
            Toggle("自动读取币安行情和交易规则", isOn: $model.form.live)
            if !model.form.live { rows([.price]) }
        }.padding(8)
    }

    /// 输入：当前区间模型；返回：该模型专用参数，隐藏字段不传到错误模式。
    private var strategy: some View {
        VStack(alignment: .leading, spacing: 11) {
            gridSettings
            Picker("选参算法", selection: binding(.algorithm)) {
                Text("自适应 · 历史验证").tag("adaptive")
                Text("经典 · 最多可行格数").tag("classic")
            }.accessibilityIdentifier("algorithm")
            Text(model.form.value(.algorithm) == "adaptive"
                 ? "自适应需要联网 ATR 历史、至少 120 根。程序比较区间宽度和格数；证据不足会建议观望，SL/TP 请使用 ATR 倍数。"
                 : "经典算法按资金和风险约束选择最多格数，支持手动输入。")
                .font(.caption).foregroundStyle(.secondary)
            Picker("区间模型", selection: $model.form.range) {
                ForEach(RangeChoice.allCases, id: \.self) { Text($0.title).tag($0) }
            }.pickerStyle(.segmented).labelsHidden()
            switch model.form.range {
            case .percent:
                rows([.down, .up])
                Text("按现价向两侧展开；百分比是你的区间假设。")
                    .font(.caption).foregroundStyle(.secondary)
            case .manual:
                rows([.lower, .upper])
                Text("填入你确认的箱体边界，现价需要位于区间内。")
                    .font(.caption).foregroundStyle(.secondary)
            case .atr:
                if !model.form.live {
                    rows([.atr, .atrPeriod])
                    intervalPicker
                }
                rows([.rangeAtr, .stopAtr, .takeAtr])
                Text("联网使用已收盘 K 线；手填 ATR 请与选定周期一致。")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }.padding(8)
    }

    /// 输入：网格类型与格数草稿；返回：始终可见的类型、搜索上限和固定值选项。
    private var gridSettings: some View {
        VStack(alignment: .leading, spacing: 11) {
            Picker("网格类型", selection: binding(.gridMode)) {
                Text("等比 · 相同比例").tag("geometric")
                Text("等差 · 相同价差").tag("arithmetic")
            }.accessibilityIdentifier("grid-mode")
            rows([.maxGrids, .grids])
            Text("固定格数留空时自动选择；自适应按历史评分选优，经典选最多可行格数。更多格会缩小价差并分散每格资金，不保证收益更高。")
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    /// 输入：周期及根数；返回：独立获取真实历史的入口，不要求先填完资金。
    private var historicalSettings: some View {
        VStack(alignment: .leading, spacing: 11) {
            intervalPicker
            rows([.historyBars, .atrPeriod])
            Button(action: model.fetchHistory) {
                Label("获取历史行情", systemImage: "arrow.down.circle")
            }.accessibilityIdentifier("fetch-history")
            Text("默认最近 180 根已收盘日线。先获取可查看走势与 ATR；生成方案时会重新获取最新数据。")
                .font(.caption).foregroundStyle(.secondary)
            Text("历史根数须大于 ATR 周期，最多 1000。所有联网请求使用下方代理设置。")
                .font(.caption).foregroundStyle(.secondary)
        }.padding(8)
    }

    /// 输入：interval 草稿；返回：与后端支持范围一致的固定周期选择器。
    private var intervalPicker: some View {
        Picker("K 线周期", selection: binding(.interval)) {
            ForEach(["1m", "5m", "15m", "30m", "1h", "4h", "1d", "1w"], id: \.self) { Text($0).tag($0) }
        }
    }

    /// 输入：代理开关/协议；返回：协议、主机与端口输入区。
    private var proxy: some View {
        VStack(alignment: .leading, spacing: 11) {
            Toggle("使用指定代理", isOn: $model.form.useProxy)
            if model.form.useProxy {
                Picker("协议", selection: $model.form.proxy) {
                    ForEach(ProxyChoice.allCases, id: \.self) { Text($0.title).tag($0) }
                }
                rows([.proxyHost, .proxyPort])
            }
            Text("代理只用于本应用的联网行情请求。地址与端口以你的代理软件为准。")
                .font(.caption).foregroundStyle(.secondary)
        }.padding(8)
    }

    /// 输入：成本、风险及覆盖价草稿；返回：可展开的高级参数区。
    private var advanced: some View {
        VStack(alignment: .leading, spacing: 11) {
            rows([.fee, .slippage, .minOrder, .minNet, .reserve, .stress])
            if model.form.range != .atr { rows([.outside]) }
            rows([.stop, .take])
            Text("百分比直接填数值：0.1 表示 0.1%。手填 SL 需低于下限，TP 需高于上限。")
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    /// 输入：字段列表；返回：复用的表单行，每项通过标识读写同一份草稿。
    private func rows(_ fields: [FieldID]) -> some View {
        ForEach(fields, id: \.self) { field in // 当前字段规格决定标题、占位符、绑定及无障碍标识。
            HStack(spacing: 8) {
                Text(field.spec.title).font(.system(size: 12)).frame(width: 135, alignment: .leading)
                TextField(field.spec.placeholder, text: binding(field))
                    .textFieldStyle(.roundedBorder).font(.system(size: 12, design: .monospaced))
                    .accessibilityLabel(field.spec.title).accessibilityIdentifier(field.rawValue)
            }
        }
    }

    /// 输入：字段标识；返回：双向绑定，编辑后模型会清除过期方案。
    private func binding(_ field: FieldID) -> Binding<String> {
        Binding(get: { model.form.values[field.rawValue] ?? field.spec.initial },
                set: { model.form.values[field.rawValue] = $0 })
    }
}

/// 结果展示面板；方案、真实历史及错误均读取共享模型，不在视图里重新计算。
@MainActor
struct ResultPane: View {
    /// 共享的主线程状态；ObservedObject 在表单或结果变化时刷新视图。
    @ObservedObject var model: PlannerModel
    /// 输入：当前共享模型/卡片属性；返回：SwiftUI 主布局，不发起隐式请求。
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                if let error = model.error { // 后端/校验错误保持可选择复制，便于定位失败。
                    Label("无法完成请求", systemImage: "exclamationmark.triangle.fill")
                        .font(.headline).foregroundStyle(.red)
                    Text(error).font(.callout).textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12).background(.red.opacity(0.07), in: RoundedRectangle(cornerRadius: 10))
                }
                if let result = model.result { report(result.plan) } // result 是已校验方案，界面直接展示后端结果。
                if let data = model.history { historicalReport(data.history) } // data 是本次真实历史快照，图表不会读取 demo CSV。
                if model.result == nil && model.history == nil && model.error == nil { empty }
            }.padding(20).frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    /// 输入：是否运行中；返回：等待提示或初始说明，保留风险边界提醒。
    private var empty: some View {
        VStack(spacing: 15) {
            Image(systemName: model.running ? "network" : "slider.horizontal.3")
                .font(.system(size: 42, weight: .light)).foregroundStyle(.secondary)
            Text(model.running ? model.activity : "从左侧填写信息开始").font(.title3.weight(.semibold))
            Text(model.running ? "读取行情需要一点时间，可以随时取消。" : "生成后查看上下限、格数、投入和止盈止损，再复制到币安手动网格。")
                .font(.callout).foregroundStyle(.secondary).multilineTextAlignment(.center)
            Text("止损预算是情景估算，实际成交可能造成更大亏损。")
                .font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.center)
        }.padding(.vertical, 100).padding(.horizontal, 16).frame(maxWidth: .infinity)
    }

    /// 输入：已收盘历史快照；返回：真实收盘价曲线、样本范围、波幅与完整导出入口。
    private func historicalReport(_ h: HistoryData) -> some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                Text("\(h.candleInterval) · \(h.closedCandleCount) 根已收盘 K 线 · 收盘价走势")
                    .font(.caption).foregroundStyle(.secondary)
                Chart(h.candles) { candle in // 每个点直接取真实样本的开盘时间与收盘价。
                    LineMark(x: .value("时间", candle.date), y: .value("收盘价", candle.close))
                        .foregroundStyle(.blue)
                }.chartYScale(domain: .automatic(includesZero: false)).frame(height: 185)
                    .accessibilityLabel("\(h.symbol) 已收盘历史价格走势")
                metric("当前公开报价", "\(money(h.currentPrice)) USDT")
                metric("Wilder ATR(\(h.atrPeriod))", "\(money(h.atr)) USDT")
                if let low = h.candles.map(\.low).min(), let high = h.candles.map(\.high).max() { // 样本极值，仅用于描述历史范围。
                    metric("样本最低 / 最高", "\(money(low)) / \(money(high))")
                }
                metric("历史开始", timestamp(h.firstCandleOpenMs))
                metric("末根收盘", timestamp(h.lastCandleCloseMs))
                metric("获取时服务器时间", timestamp(h.marketAsOfMs))
                Text("以上时间按本地时区显示；K 线边界按 UTC。历史高低点不自动视为支撑阻力。")
                    .font(.caption).foregroundStyle(.secondary)
                Text("来源：\(h.sourceUrl)").font(.caption2).foregroundStyle(.secondary).textSelection(.enabled)
                ForEach(h.warnings, id: \.self) { Text($0).font(.caption).foregroundStyle(.orange) }
                DisclosureGroup("最近 20 根 · 开 / 高 / 低 / 收") {
                    VStack(alignment: .leading, spacing: 8) {
                        ForEach(Array(h.candles.suffix(20))) { c in // c 是最近 20 根中的当前真实 OHLC。
                            Text("\(timestamp(c.openTime))\n\(money(c.open)) / \(money(c.high)) / \(money(c.low)) / \(money(c.close))")
                                .font(.system(.caption2, design: .monospaced)).textSelection(.enabled)
                        }
                    }.padding(.top, 8)
                }
            }.padding(8)
        } label: {
            HStack {
                Label("历史行情 · \(h.symbol)", systemImage: "chart.xyaxis.line")
                Spacer()
                Button("导出历史 JSON", action: model.exportHistory).font(.caption)
            }
        }
    }

    /// 输入：真实计算结果；返回：线位、资金和风险摘要，复制字段与显示结果保持一致。
    private func report(_ p: PlanData) -> some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                Label(p.symbol, systemImage: "checkmark.circle.fill").font(.title3.weight(.semibold))
                Spacer()
                Text("现货 · \(p.gridModeTitle)").font(.caption).foregroundStyle(.secondary)
            }
            Text("参考现价 \(p.referencePrice) USDT").font(.caption).foregroundStyle(.secondary)
            if let audit = p.optimization { optimization(audit) }
            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 12) {
                ValueCard(title: "区间下限", value: p.lowerPrice, tint: .blue)
                ValueCard(title: "区间上限", value: p.upperPrice, tint: .blue)
                ValueCard(title: "止损 SL", value: p.stopLoss, tint: .red)
                ValueCard(title: "停止价 TP", value: p.takeProfit, tint: .green)
            }
            GroupBox("币安填写参数") {
                VStack(spacing: 10) {
                    metric("网格数量", "\(p.gridCount) 格 · \(p.gridPrices.count) 个价格点")
                    metric("投入金额", "\(money(p.investmentUsdt)) USDT")
                    metric("未分配资金", "\(money(p.unallocatedUsdt)) USDT")
                    metric("停止时卖出全部基础币", "开启")
                    metric("每格数量估算", p.quantityPerGrid)
                    Text(p.gridCountExplanation).font(.caption).foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }.padding(8)
            }
            risk(p)
            DisclosureGroup("网格价格明细") {
                Text(p.gridPrices.joined(separator: "\n")).font(.system(.caption, design: .monospaced))
                    .textSelection(.enabled).padding(.top, 8)
            }
            DisclosureGroup("计算边界与提醒") {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(p.warnings, id: \.self) { Text("• \($0)").font(.caption).foregroundStyle(.secondary) }
                }.padding(.top, 8)
            }
        }.textSelection(.enabled)
    }

    /// 输入：方案；返回：含止损和压力情景的风险面板，不显示盈利保证。
    private func risk(_ p: PlanData) -> some View {
        GroupBox("资金与风险估算") {
            VStack(spacing: 10) {
                metric("账户风险预算", "\(money(p.riskBudgetUsdt)) USDT")
                metric("单边下跌止损亏损", "\(money(p.stopScenarioLossUsdt)) USDT")
                metric("跌穿止损压力亏损", "\(money(p.stressScenarioLossUsdt)) USDT")
                metric("最差一格净收益", "\(String(format: "%.3f", p.worstNetGridPct))%")
                metric("手续费预留", "\(money(p.feeReserveUsdt)) USDT")
                if let atr = p.atr { metric("ATR · \(p.candleInterval)", money(atr)) } // 只有计算了 ATR 才展示。
                metric("数据来源", p.dataSource == "binance_public_api" ? "币安公开行情" : "手动填写")
                Text("以完整成交及假设成本计算，实际亏损可能超过预算。创建前核对币安预览。")
                    .font(.caption).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading)
            }.padding(8)
        }
    }

    /// 输入：后端选参审计；返回：判定、独立最终检验及经典/买入持有对照，收益按总本金计算。
    private func optimization(_ report: OptimizationData) -> some View {
        GroupBox(report.recommendation == "wait" ? "建议观望 · 下方参数仅供诊断" : "自适应 · 历史门槛通过") {
            VStack(alignment: .leading, spacing: 10) {
                metric("区间 ATR 倍数", money(report.selectedRangeAtrMult))
                metric("可行 / 搜索组合", "\(report.feasibleCandidates) / \(report.testedCandidates)")
                metric("发展验证 / 风险调整评分", "\(report.developmentFolds.count) 段 / \(money(report.developmentScore))")
                metric("最终检验", "\(report.holdout.evaluatedBars) 根（未参与选参）")
                Text("\(timestamp(report.holdout.evaluationStartOpenMs)) — \(timestamp(report.holdout.evaluationEndOpenMs))")
                    .font(.caption).foregroundStyle(.secondary)
                metric("最终检验本金净收益", "\(money(report.holdout.candidate.netReturnPct))%")
                metric("最大权益回撤", "\(money(report.holdout.candidate.maxDrawdownPct))%")
                metric("双倍成本本金净收益", "\(money(report.holdout.candidate.stressNetReturnPct))%")
                metric("完成网格卖出", "\(report.holdout.candidate.completedCycles) 次")
                if let baseline = report.holdout.baseline {
                    metric("经典算法同段净收益", "\(money(baseline.netReturnPct))%")
                }
                metric("同额投入买入持有", "\(money(report.holdout.buyHoldReturnPct))%")
                Text(report.reason).font(.caption)
                    .foregroundStyle(report.recommendation == "wait" ? .orange : .secondary)
                Text("历史检验不能保证未来收益。买入持有的风险敞口与网格不同。")
                    .font(.caption).foregroundStyle(.secondary)
            }.padding(8)
        }
    }

    /// 输入：标签与数值；返回：可选择复制、两端对齐的摘要行。
    private func metric(_ title: String, _ value: String) -> some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title).font(.system(size: 12)).foregroundStyle(.secondary)
            Spacer(minLength: 10)
            Text(value).font(.system(size: 12, weight: .medium, design: .monospaced)).multilineTextAlignment(.trailing)
        }
    }
}

/// 复用的价格摘要卡，显示标题、精确字符串和颜色，四个主要线位共用此布局。
struct ValueCard: View {
    /// 卡片标题，例如区间下限或止损 SL。
    let title: String
    /// 后端精确价格字符串，界面不重新改变小数精度。
    let value: String
    /// 强调颜色；只影响展示，不改变策略或风险含义。
    let tint: Color
    /// 输入：当前共享模型/卡片属性；返回：SwiftUI 主布局，不发起隐式请求。
    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(title).font(.caption).foregroundStyle(.secondary)
            Text(value).font(.system(size: 18, weight: .semibold, design: .monospaced))
                .foregroundStyle(tint).lineLimit(1).minimumScaleFactor(0.65)
            Text("USDT").font(.system(size: 10)).foregroundStyle(.secondary)
        }.frame(maxWidth: .infinity, alignment: .leading).padding(14)
            .background(tint.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
    }
}
