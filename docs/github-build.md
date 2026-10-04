# 用 GitHub 自动打包 macOS 应用

把项目推送到 GitHub 后，GitHub Actions 会使用两台 macOS 构建机器，分别生成 Apple Silicon 和 Intel 应用。下载、解压后得到 `Grid Planner.app`，运行时不需要安装 Rust 或 Xcode。

## 先明确自动打包要避免哪些失败

以下验收条件在添加工作流前确定，后续使用现有端到端测试和真实应用打包验证。

| 失败情景 | 工作流的处理方式 |
| --- | --- |
| 代码上传了，但遗漏 `Cargo.lock` | 必须将锁文件提交；使用 `--locked`，不在构建时偷偷换依赖版本 |
| Apple Silicon / Intel 构建机器选错 | 分别使用明确的 runner 标签，并核对 `uname -m` 与目标架构 |
| 缺少 Swift / Rust 工具链或版本不兼容 | 显示工具版本，安装稳定 Rust 及 rustfmt / Clippy，Swift 使用版本 6 语言模式 |
| 编译或测试失败 | 立即失败，不上传看似成功的安装包 |
| 只有前端，没有包内 Rust 程序 | 桌面端到端测试调用 `.app` 内真实后端；缺失就失败 |
| 原始 `.app` 上传后丢失可执行权限 | 先用 macOS 的 `ditto` 打成 ZIP，再上传 ZIP，而不是逐个上传应用文件 |
| 应用签名或 Info.plist 异常 | 本地构建脚本执行 plist 和严格签名检查 |
| 上传路径写错、构建产物不存在 | 上传步骤设置 `if-no-files-found: error` |
| 文件下载后损坏 | 同时提供 SHA-256 校验文件，可核对下载完整性 |
| 云构建机无法访问币安或没有代理 | 验收只使用本地 HTTP / SOCKS5h 模拟服务，不依赖实时行情或交易账户 |
| 用户只下载到 GitHub 的外层压缩包 | 按下文步骤解压两层，最后运行 `.app` |

## 1. 准备 GitHub 仓库

在 GitHub 新建一个仓库，例如 `grid-planner`。首次推送时，空仓库更方便，不必让 GitHub 额外创建 README。

应提交源码、`Cargo.toml`、**`Cargo.lock`**、`.cargo`、`desktop`、`scripts`、`docs`、`examples` 和 `.github/workflows/build-macos.yml`。`target` 和 `dist` 是构建结果，已通过 `.gitignore` 排除。锁文件必须提交，否则 `cargo --locked` 无法在干净的构建机上重现依赖。

当前项目已经是 Git 仓库。以下命令在项目根目录运行；先把示例 URL 换成你自己的仓库地址：

```bash
git add .
git commit -m "docs: add Chinese comments and macOS build workflow"
git remote add origin https://github.com/YOUR_ACCOUNT/grid-planner.git
git push -u origin HEAD
```

若已有 `origin`，不必重复添加，可用 `git remote -v` 查看。工作流支持向 `main`、`master` 推送、向仓库提交 Pull Request、推送 `v*` 标签，以及在 GitHub 页面手动运行。若你的主分支叫其他名字，在工作流的 `push.branches` 中加入它。

## 2. 查看自动构建

打开仓库的 **Actions** 页，选择 **构建 macOS 应用**，进入最新一次运行。

两项构建使用相同源码和现有脚本，依次执行格式检查、Clippy、20 项 Rust CLI 端到端验收、应用构建及签名检查、11 项桌面桥接端到端验收，最后上传安装包。绿色勾表示该架构的步骤全部成功。

| 你的 Mac | 构建机器 | 应下载的 Artifact | 内部安装包 |
| --- | --- | --- | --- |
| Apple M 系列芯片 | `macos-15`，arm64 | `macOS-app-arm64` | `GridPlanner-macOS-arm64.zip` |
| Intel 处理器 | `macos-15-intel`，x86_64 | `macOS-app-x86_64` | `GridPlanner-macOS-x86_64.zip` |

可在 Mac 左上角「苹果菜单 → 关于本机」查看芯片 / 处理器。上述 runner 标签和架构已按 [GitHub 官方运行器文档](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)核对，不能把 `macos-latest` 永远当成 Intel。

## 3. 手动点一下也能打包

确保工作流文件已经进入仓库的默认分支。进入 **Actions → 构建 macOS 应用 → Run workflow**，选择分支并确认。无需自己准备 Mac 构建服务器，也无需填写币安 API Key、代理地址或苹果开发者证书。

看不到 **Run workflow** 时，先检查工作流是否在默认分支、你是否拥有运行权限，以及仓库的 Actions 是否被禁用。手动运行对应工作流中的 `workflow_dispatch` 入口。操作依据见 [GitHub 手动运行文档](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)。

## 4. 下载并打开应用

1. 进入成功的运行记录，在底部 **Artifacts** 下载与你的 Mac 架构对应的文件。通常需要登录 GitHub，并拥有该仓库的读取权限。
2. 解压 GitHub 下载的外层 ZIP，里面是 `GridPlanner-macOS-架构.zip` 和同名 `.sha256` 校验文件。
3. 再解压内部安装包，得到 **`Grid Planner.app`**。
4. 把 `.app` 拖入「应用程序」，双击启动。顶部出现「网格」，点击菜单打开参数面板。

只复制 `.app` 内某个可执行文件会丢失资源和计算组件；请移动完整应用。界面、Rust 后端、README、示例配置和文档都已装进应用包。

GitHub Artifact 的外层包装与下载权限见[官方下载说明](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/download-workflow-artifacts)。使用内层 ZIP 是为了保留 macOS 应用的文件布局和可执行权限，依据见 [upload-artifact 的权限说明](https://github.com/actions/upload-artifact#permission-loss)。

若需要核对文件完整性，在内层 ZIP 和 `.sha256` 所在目录运行：

```bash
shasum -a 256 -c GridPlanner-macOS-arm64.zip.sha256
```

Intel 版把文件名中的 `arm64` 换成 `x86_64`。出现 `OK` 表示文件和这次构建提供的校验值一致；校验值不等于苹果公证或发布者身份认证。

## 5. macOS 提示无法验证开发者怎么办

当前脚本做的是 **ad-hoc 本地签名**，用于生成完整的可执行应用；它没有使用 Apple Developer ID，也没有进行苹果公证。从 GitHub 下载后，Gatekeeper 仍可能拦截这种应用。

如果你确认仓库和此次构建来源可信，先尝试打开应用，再进入「系统设置 → 隐私与安全性」查看系统提供的「仍要打开」选项。不要关闭整个系统的安全检查。具体流程以[苹果官方说明](https://support.apple.com/zh-cn/102445)为准。

如果要公开分发，并希望陌生用户正常通过 Gatekeeper，需要另外准备 Apple Developer ID 证书和公证凭据，改为 Developer ID 签名、提交 `notarytool` 公证并装订票据。当前工作流不需要这些凭据，也不能把 ad-hoc 签名检查通过称为「苹果认证」。苹果的正式发行流程见[公证文档](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)。

## 6. 工作流和脚本分别负责什么

| 文件 | 作用 |
| --- | --- |
| `.github/workflows/build-macos.yml` | 决定何时运行、两种架构的构建机器、检查顺序和 Artifact 上传 |
| `scripts/build-macos.sh` | 编译 Rust / Swift，装配 `.app`，复制资源，签名，生成安装 ZIP 和校验文件 |
| `scripts/test-macos.sh` | 编译桌面桥接验收程序，调用已打包应用中的真实 Rust 后端 |
| `desktop/macos/Info.plist` | 应用名、包标识、版本、macOS 最低版本和菜单栏驻留方式 |
| `Cargo.lock` | 锁定 Rust 依赖，供本地和 GitHub 使用相同的依赖版本 |

工作流使用只读仓库权限，不自动发布 GitHub Release。Artifact 在此配置中保留 14 天。想提供长期下载，可从成功记录下载两个安装 ZIP 及校验文件，再上传到 GitHub **Releases** 的对应版本。

工作流中的官方 Actions 固定到完整提交号，旁边注明版本，避免浮动标签变化悄悄改变构建行为。本次按官方发行记录核对了 [checkout v7.0.1](https://github.com/actions/checkout/releases/tag/v7.0.1)、[cache v6.1.0](https://github.com/actions/cache/releases/tag/v6.1.0) 和 [upload-artifact v7.0.1](https://github.com/actions/upload-artifact/releases/tag/v7.0.1)。日后升级 Actions 时，应同时更新提交号和版本注释。Rust 工具链使用当时的稳定版，`Cargo.lock` 锁定的是依赖解析结果，不是编译器版本。

Rust 与 Swift 的部署目标均设置为 macOS 13；两种架构各自编译，不是单个 Universal 通用二进制。云端验收覆盖命令行与桌面桥接链路，不会在虚拟机桌面自动点击菜单或验证真实下单，也不证明 macOS 13 真机的全部兼容性。

## 7. 失败时从哪里看

进入红色失败记录，展开第一个失败步骤即可定位原因：

- `Cargo.lock` 缺失 / 需要更新：确认锁文件已提交；改依赖后在本地正常更新它，再重新提交。
- Swift 语言版本不支持：查看「检查构建环境」的 Xcode / Swift 版本，确认 runner 使用支持 Swift 6 的 Xcode。
- 下载依赖失败：网络问题可重跑失败任务；构建没有偷偷更换镜像或回退依赖版本。
- 端到端验收失败：查看具体 Rust / Swift 错误；通过验收前不会上传安装包。
- 签名验证失败或安装包不存在：检查「构建应用」的日志及脚本中的打包路径。
- Artifact 已过期：重新手动运行，或从自己维护的 Release 下载长期保留版本。

## 8. 本次本地验证

2026-10-04 在 Apple Silicon Mac 上完成：

- 工作流通过 `actionlint v1.7.12`，打包/验收脚本通过 Bash 语法检查。
- Rust 格式检查、Clippy 和现有 20 项 CLI 端到端验收通过。
- 原生应用构建及严格签名验证通过；现有 11 项桌面桥接端到端验收通过。
- Rust 与 Swift 二进制的 Mach-O 最低系统目标均为 13.0；ZIP 内容、可执行权限和 `.sha256` 校验通过。

本次没有把代码推送到 GitHub，没有执行在线 Actions，也没有在 Intel 或 macOS 13 真机上运行。参考资料核对日期为 2026-10-04；在线构建成功与否应以提交后实际 Actions 记录为准。
