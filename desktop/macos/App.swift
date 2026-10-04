// 应用入口与菜单栏/窗口生命周期。

import AppKit
import SwiftUI

/// 原生 macOS 入口，创建 AppKit 事件循环，并由菜单代理管理窗口生命周期。
@main
struct GridPlannerApp {
    /// 输入：应用启动事件；返回：无；运行原生菜单栏应用事件循环。
    @MainActor static func main() {
        let app = NSApplication.shared // 共享 NSApplication，accessory 模式只驻留菜单栏。
        let delegate = MenuDelegate() // 强引用代理，NSApplication.delegate 不负责延长其生命周期。
        app.delegate = delegate
        app.setActivationPolicy(.accessory)
        app.run()
        withExtendedLifetime(delegate) {} // 覆盖整个事件循环生命周期，保证代理始终存活。
    }
}

/// 主线程应用代理，强引用菜单栏入口、窗口及共享表单模型。
/// 关闭面板仅隐藏窗口；退出应用才取消任务并结束事件循环。
@MainActor
final class MenuDelegate: NSObject, NSApplicationDelegate {
    /// 菜单栏项的强引用；不持有时系统可能移除菜单入口。
    private var statusItem: NSStatusItem?
    /// 复用的参数窗口；关闭后保留，重新打开时继续使用同一份表单。
    private var window: NSWindow?
    /// 整个应用共享的主线程模型，避免多次打开窗口生成独立状态。
    private let model = PlannerModel()

    /// 输入：启动通知；返回：无；创建菜单栏入口及首次显示的参数窗口。
    func applicationDidFinishLaunching(_ notification: Notification) {
        createStatusItem()
        createEditMenu()
        showWindow()
    }

    /// 输入：无；返回：无；创建强引用持有的状态栏图标和打开/退出菜单。
    private func createStatusItem() {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength) // 可变宽度的状态栏入口，最后保存在 statusItem。
        let icon = NSImage(systemSymbolName: "chart.line.uptrend.xyaxis", accessibilityDescription: "网格交易助手") // 系统符号图标，模板模式自动适配明暗菜单栏。
        icon?.isTemplate = true
        item.button?.image = icon
        item.button?.title = " 网格"
        item.button?.toolTip = "网格交易助手"
        let menu = NSMenu() // 点击状态栏项展示的原生菜单。
        let show = NSMenuItem(title: "打开参数面板", action: #selector(showWindow), keyEquivalent: "") // 打开同一参数窗口的菜单操作，target 显式指向当前代理。
        show.target = self
        menu.addItem(show)
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "退出网格交易助手", action: #selector(quit), keyEquivalent: "q") // 退出入口，同时取消当前后端请求。
        quit.target = self
        menu.addItem(quit)
        item.menu = menu
        statusItem = item
    }

    /// 输入：无；返回：无；重新显示同一窗口，关闭窗口不丢失填写内容。
    @objc private func showWindow() {
        if window == nil { createWindow() }
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }

    /// 输入：无；返回：无；用 SwiftUI 表单创建可调整大小的原生窗口。
    private func createWindow() {
        let panel = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 910, height: 760), // 承载 SwiftUI 的原生窗口，尺寸允许两栏完整显示。
                             styleMask: [.titled, .closable, .miniaturizable, .resizable],
                             backing: .buffered, defer: false)
        panel.title = "网格交易助手"
        panel.minSize = NSSize(width: 840, height: 600)
        panel.contentView = NSHostingView(rootView: PlannerView(model: model))
        panel.isReleasedWhenClosed = false // 关闭窗口时保留实例，菜单可重新显示而无需重建草稿。
        panel.center()
        window = panel
    }

    /// 输入：无；返回：无；补全原生文本输入的撤销、复制、粘贴与全选快捷键。
    private func createEditMenu() {
        let main = NSMenu() // 主菜单，提供原生应用/编辑命令的响应链入口。
        let appItem = NSMenuItem() // 应用子菜单的父项。
        let appMenu = NSMenu() // 包含退出命令的应用子菜单。
        let quit = NSMenuItem(title: "退出网格交易助手", action: #selector(quit), keyEquivalent: "q") // 退出入口，同时取消当前后端请求。
        quit.target = self
        appMenu.addItem(quit)
        appItem.submenu = appMenu
        main.addItem(appItem)
        let editItem = NSMenuItem(title: "编辑", action: nil, keyEquivalent: "") // 编辑子菜单的父项。
        let edit = NSMenu(title: "编辑") // 撤销、剪切、复制、粘贴、全选通过原生响应链处理。
        for (title, selector, key) in [("撤销", "undo:", "z"), ("剪切", "cut:", "x"), // 逐项配置显示名、响应链 selector 及 Command 快捷键。
                                      ("复制", "copy:", "c"), ("粘贴", "paste:", "v"), ("全选", "selectAll:", "a")] {
            edit.addItem(NSMenuItem(title: title, action: NSSelectorFromString(selector), keyEquivalent: key))
        }
        editItem.submenu = edit
        main.addItem(editItem)
        NSApp.mainMenu = main
    }

    /// 输入：应用实例；返回：false，让关闭最后一个窗口后菜单栏入口继续保留。
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }

    /// 输入：无；返回：无；取消请求后明确退出应用。
    @objc private func quit() { model.cancel(); NSApp.terminate(nil) }

    /// 输入：退出通知；返回：无；处理其他方式退出时的子进程清理。
    func applicationWillTerminate(_ notification: Notification) { model.cancel() }
}
