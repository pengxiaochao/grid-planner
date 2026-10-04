# 币安现货网格参数与历史验证助手

填入资金、现价和风险限制，自动生成**区间下限、上限、网格数量、投入金额、止损 SL、停止价 TP**。结果可以抄到币安的「现货网格 → 手动」页面，网格类型按报告选择等比或等差。

程序支持 USDT 报价的现货交易对，例如 BTCUSDT、ETHUSDT。它生成参数，不持有 API Key，不连接账户、不创建机器人、不下单。资金模型假设从 USDT 开始、参考价位于区间内部、使用等量基础币的静态等比或等差网格。

**V0.4.0 新增等差选项与格数解释**：桌面“网格类型”可选等比/等差，CLI 使用 `--grid-mode geometric|arithmetic`，默认保留等比。两种类型共用资金、交易规则及历史验证；搜索上限与固定格数在主设置区可见，结果解释为什么选择当前格数。截图现象、成本计算及本版验收见 [等差/等比与格数说明](docs/grid-modes-verification.md)。

**V0.3.0 新增自适应选参**：比较 ATR 区间宽度和格数，用三段滚动发展验证评价扣费收益、库存回撤及双倍成本压力，再用未参与选参的最后 20% 历史检验。证据不足显示“观望”；原资金与交易规则约束继续生效。桌面默认新版，CLI 默认 `classic` 保留旧命令行为。

收益不能保证最大化。新版优化明确候选集合中的历史风险调整评分，不保证未来收益，也不声称优于所有策略。开源比较、公式及边界见 **[V0.3 自适应算法介绍](docs/adaptive-grid.md)**，实际验收及真实行情比较见 [本版验证记录](docs/adaptive-verification.md)。

```bash
cargo run --locked -- --capital 600 --live --mode atr --algorithm adaptive --history-bars 180
cargo run --locked -- --config examples/adaptive.toml --live
cargo run --locked -- --capital 600 --live --mode atr --algorithm adaptive --grid-mode arithmetic
```

## 1. 小白先这样用

### macOS 菜单栏应用

需要 macOS 13 或更高版本。双击项目中的 `dist/Grid Planner.app`，顶部菜单栏会出现「网格」，点击「打开参数面板」即可使用。当前打包文件是 Apple Silicon 版本；运行应用无需安装 Rust。

1. 填写**最多投入**、**账户资产**和**风险百分比**。账户资产留空时等于最多投入。
2. 保持「自动读取币安行情和交易规则」开启，交易对默认 `BTCUSDT`，区间默认 ATR，算法默认「自适应 · 历史验证」。选择「网格类型」等比或等差；「固定格数」留空时自动选择。百分比、手动区间或手填 ATR 请选经典算法。
3. 如果需要代理，开启「使用指定代理」，选 HTTP 或 SOCKS5，填代理软件的 IP 和端口，例如 `127.0.0.1` / `7890`。端口以你的实际配置为准。
4. 在「真实历史行情」选周期和根数，点击**获取历史行情**。默认获取最近 **180 根已收盘日线**，面板会显示收盘价走势、ATR、样本高低点和获取时间；此步骤不需要先填完资金。
5. 点击**生成网格方案**。应用重新读取最新报价、规则和历史，显示参数及验证结果。自适应至少需 120 根，大 ATR 周期还需更长预热。“观望”时参数供诊断并禁用复制；“历史门槛通过”时再核对参数与币安创建预览。

「导出历史 JSON」保存整批 OHLC、ATR、数据来源与时间；「导出 JSON」保存完整方案及其历史依据。修改资金会清除旧方案；修改交易对、周期、根数或代理还会清除旧历史快照。网络失败会显示错误，不会读取 demo CSV 来凑出结果。

关闭窗口后应用继续驻留菜单栏；从顶部「网格 → 退出」彻底退出。输入和代理设置会保存在本机，启动时不会自动联网或生成方案。

桌面界面使用原生 SwiftUI / AppKit，计算和行情请求全部由包内 Rust 程序完成。开发者可在 Mac 上构建自己的架构版本：

```bash
bash scripts/build-macos.sh
```

输出 `.app`、`dist/GridPlanner-macOS-arm64.zip` 和同名 `.sha256` 校验文件（Intel Mac 的文件名为 `x86_64`）。构建需要 Rust 和 Xcode Command Line Tools；离线依赖已缓存时可设置 `GRID_BUILD_OFFLINE=1`。可以把 `.app` 拖到「应用程序」文件夹。

不想在本机安装编译工具，可以把源码上传 GitHub，让 Actions 自动生成 Apple Silicon 和 Intel 两个版本。完整步骤见 **[用 GitHub 自动打包 macOS 应用](docs/github-build.md)**，包含创建仓库、手动触发、下载解压、校验和 macOS 开启应用的方法。对应配置在 [.github/workflows/build-macos.yml](.github/workflows/build-macos.yml)。

### 命令行用法

安装 [Rust](https://www.rust-lang.org/tools/install)，需要 Rust 1.85 或更高版本。进入项目目录：

```bash
cargo run --locked
```

程序依次询问：

1. **最多可投入本金**：最多拿多少 USDT 给这一个机器人。
2. **现价**：从币安看到的当前交易对价格。
3. **账户总资产**：全部交易资金折合 USDT，不只是给这个机器人的钱。
4. **风险百分比**：这一个策略的止损情景最多消耗账户资产多少。填 `2` 表示 `2%`。

现价 `84000` 仅用于下面的演示，**不是今天的 BTC 报价**。想少填一项，可让程序读取公开行情：

```bash
cargo run --locked -- --interactive --live --mode atr --history-bars 180
```

此命令读取公开价格、交易规则和 180 根已收盘日线，用 ATR 生成区间。网络访问失败会报错；需要代理时加 `--proxy-url http://127.0.0.1:7890` 或 `--proxy-url socks5h://127.0.0.1:1080`，指定代理失败不会直连回退。

### 单独获取真实历史 BTC 行情

不填写本金也能获取历史数据，默认输出 JSON，`--output` 同时保存文件：

```bash
cargo run --locked -- --fetch-history --symbol BTCUSDT \
  --interval 1d --history-bars 180 --atr-period 14 --output history.json
```

数据来自币安公开 `/api/v3/klines`，并读取公开现价、交易规则和服务器时间，全程无需 API Key。支持 `1m / 5m / 15m / 30m / 1h / 4h / 1d / 1w`。**根数不是固定天数**：180 根日线约是 180 天，180 根小时线约是 7.5 天。K 线采用默认 UTC 边界，周线从周一开始；面板时间按本地时区显示。

历史数量必须大于 ATR 周期且不超过 1000，后者对应币安单次 K 线请求上限。程序把请求截止时间设在当前 K 线之前，还逐根核对服务器时间，排除未收盘数据。API 若返回不足请求数量，会如实显示实际根数；不足 ATR 所需样本或时间有断档则报错，不补造数据。接口参数依据见[币安公开行情文档](https://developers.binance.com/en/docs/catalog/core-trading-spot-trading/api/rest-api/market)。

现价来自实时公开报价，ATR 来自已收盘历史波幅；它们不是同一个数据。曲线中的历史高低点只描述这批样本，不自动认定为支撑或阻力。180 根是便于查看和减小 ATR 初始化影响的默认值，不是经过盈利回测的最优长度。

### 离线演示：600 USDT

```bash
cargo run --locked -- --capital 600 --price 84000
```

默认账户总资产也为 600 USDT，风险比例为 2%，因此止损情景预算是 `600 × 2% = 12 USDT`。输出中关键字段如下：

| 币安/报告字段 | 此示例输出 |
| --- | --- |
| 模式 | 等比网格 |
| 下限 | 75600.00 USDT |
| 上限 | 92400.00 USDT |
| 网格数量 | 3 |
| 投入金额 | 131.81 USDT |
| 止损 SL | 73332.00 USDT |
| 停止价 TP | 95172.00 USDT |
| 每格基础币数量估算 | 0.00052 BTC |
| 单边下跌止损情景亏损 | 约 11.84 USDT |
| 跌穿止损 5% 的压力情景亏损 | 约 17.84 USDT |

**为什么没投入全部 600？** 因为“最多可投入 600”和“最多承受 12 的情景损失”是两个不同限制。全部投入可能超过风险预算，程序会缩小投入，留下 468.19 USDT 未分配。它也不会为了凑出原文的 20 格而绕过最低订单金额。

如果账户总资产实际为 3000 USDT、其中最多投入 600，应填写实际账户资产：

```bash
cargo run --locked -- --capital 600 --equity 3000 --price 84000
```

这会把风险预算改为 60 USDT。此经典示例选满足约束的最多格数，不是预测出来的“最赚钱格数”。自适应另外比较历史评分；很宽的网格可能很久才成交一次。

### 在币安里填什么

打开「交易 → 交易机器人 → 现货网格」，选择交易对、手动设置，并选择**报告中的等比或等差模式**。依次填写报告中的下限、上限、网格数量和**程序给出的投入金额**。在高级设置里填写 SL、TP，并开启「停止时卖出全部基础币」。本模型按立即启动计算，不设置额外触发价、追踪上移或杠杆。

创建预览中的实际每格数量、最低投入和手续费预留可能与本程序不同。币安的预留规则、动态价格过滤和账户已有挂单都可能影响创建结果。如果预览要求增加投入，不要直接加钱；先用新的输入重新计算。机器人停止与基础币成功卖出是两件事，清算开关及流动性影响能否真正落袋。参见[币安现货网格说明](https://www.binance.com/en/support/faq/detail/d5f441e8ab544a5b98241e00efb3a4ab)。

## 2. 三种区间生成方式

### 百分比模式：适合知道自己想做哪个箱体的用户

```bash
cargo run --locked -- --capital 600 --price 84000 --down-pct 10 --up-pct 10
```

```text
下限 = 现价 × (1 − 下跌幅度)
上限 = 现价 × (1 + 上涨幅度)
SL = 下限 × (1 − 区间外缓冲)
TP = 上限 × (1 + 区间外缓冲)
```

默认幅度为两侧 10%，区间外缓冲为 3%。这是**用户可改的假设**，程序没有从两项输入凭空找到历史支撑阻力。它适合试算，不意味着 ±10% 会包住未来行情。

### ATR 模式：让区间宽度跟随历史波动

读取公开行情：

```bash
cargo run --locked -- --capital 600 --live --mode atr --interval 1d --history-bars 180
```

也可从图表读取 ATR 值，手工填入。这里的 `2000` 是价格波幅，不是百分比：

```bash
cargo run --locked -- --capital 600 --price 84000 --mode atr --atr 2000
```

项目的 ATR 配置也直接使用真实行情：

```bash
cargo run --locked -- --config examples/atr.toml --live
```

已移除本地 demo CSV。桌面应用及上述配置不依赖本地行情文件。CLI 仍允许高级用户读取自行整理的**真实已收盘** CSV，需明确传入 `--mode atr --candles 路径`，同时提供离线现价和实际交易规则，不能与 `--live` / `--fetch-history` 混用。CSV 固定五列，数字不加引号，表头如下：

```csv
open_time,open,high,low,close
```

时间为 Unix 毫秒，必须递增、周期一致，不能重复或漏根，OHLC 要满足 `low ≤ open/close ≤ high`。ATR(14) 至少需要 15 根；更多样本可以减少平滑初始化的影响。桌面默认 180 根；CLI 的 ATR 联网模式未指定根数时请求 `max(100, 5 × ATR周期 + 1)`，最多 1000 根。CSV 末根若尚未收盘会报错。

本程序使用 **Wilder ATR**：

```text
TR = max(本根 high − low, |high − 前根 close|, |low − 前根 close|)
初始 ATR = 前 14 个 TR 的平均值
后续 ATR = (前次 ATR × 13 + 本次 TR) / 14

下限 = 现价 − 3 × ATR
上限 = 现价 + 3 × ATR
SL = 下限 − 1.5 × ATR
TP = 上限 + 1.5 × ATR
```

可用 `--range-atr-mult`、`--stop-atr-mult`、`--take-atr-mult` 修改倍数。SL 从**网格下限**往外推，确保符合币安网格的区间外停止条件，而不是从现价减去 ATR 后落到网格内部。

ATR 把跨 K 线的价格跳动也算入波幅，比一刀切的金额缓冲更能反映历史波动。它不预测方向；`3 × ATR` 也不是正态分布里的“三倍标准差”，没有 99.7% 的未来覆盖保证。日线和小时线的 ATR 不能混用，更不能把日线 ATR 简单当作未来一个月的波幅。公式依据见 [Fidelity 的 ATR 指标说明](https://www.fidelity.com/learning-center/trading-investing/technical-analysis/technical-indicator-guide/atr)。

### 手动结构模式：你确定支撑、阻力，程序负责算格数和资金

例如复现文档中的箱体，但重新核算投入与风险：

```bash
cargo run --locked -- --capital 600 --price 84000 --mode manual \
  --lower 76000 --upper 90000 --stop-loss 73800 --take-profit 92500
```

程序不会把这些线位描述成真实历史强支撑或强阻力。支撑阻力的识别需要行情和方法验证；这里允许你输入已经判断过的位置。自动停止线也可用 `--stop-loss`、`--take-profit` 覆盖，但必须在区间之外。

## 3. 格数为什么这样计算

### 等差与等比如何分布价位

| 类型 | 价位公式，i = 0..N | 特点 |
| --- | --- | --- |
| 等比 `geometric` | `p_i = 下限 × (上限 / 下限)^(i/N)` | 相邻价格比例一致，逐格收益率接近一致 |
| 等差 `arithmetic` | `p_i = 下限 + i × (上限 − 下限) / N` | 相邻绝对价差一致，高价处收益率更低 |

程序先按交易 tickSize 取整，再逐格复算收益与订单约束。等差仍使用统一基础币数量；相同价差不意味着相同百分比收益。两种模式的资金、库存、止损及回放模型相同，网格类型由用户选择，优化器只在所选类型中比较区间与格数。定义见[币安现货网格说明](https://www.binance.com/en-NZ/support/faq/detail/d5f441e8ab544a5b98241e00efb3a4ab)。

```bash
cargo run --locked -- --config examples/arithmetic.toml
cargo run --locked -- --capital 600 --price 84000 --grid-mode arithmetic
```

默认等比公式：

```text
r = (上限 / 下限)^(1/N)
第 i 个价格 = 下限 × r^i，i = 0..N
```

**N 格有 N+1 个价格点。** 先按交易对的 tickSize 取整，再校验全部相邻价位。取整后每格收益略有差别，因此报告取其中最差的一格。

### 双边成本只扣买卖各一次

设 `b` 为买入价、`s` 为卖出价、`f` 为每边成本比例，统一基础币数量为 `q`：

```text
买入成本 = q × b × (1 + f)
卖出收入 = q × s × (1 − f)
每格净利润 / 裸买入金额 = (1 − f) × s/b − 1 − f
```

这与[币安等比网格收益公式](https://www.binance.com/en/support/faq/detail/688ff6ff08734848915de76a07b953dd)的口径一致。报告的收益率以 `q × b` 为分母；若以包含手续费的实际支出为分母，再除以 `1+f`。

程序里的 `f = (fee_pct + slippage_pct) / 100`。默认每边手续费 0.1%，另加每边 0.05% 的成本余量。额外成本作为比例费用近似处理，不是订单簿滑点预测。你应填自己实际账户、交易对和折扣条件对应的费率；maker/taker 不同，可用较高者做保守估算。程序不默认你持有 BNB，也不查询账户费率。

默认要求最差一格净收益至少 0.3%。这个正收益余量可以过滤“几乎只赚手续费”的密网，但 0.3% 仍是可调设计参数，不是经过收益回测的最佳值。原文把“双边费率”再乘二、净收益又只扣一次的口径不一致，这里统一成**每边输入、买卖各一次**。

### 不能只拿 `本金 / 30` 算数量

现货网格需要给下方买单留 USDT，也要先买基础币来挂上方卖单。币安按基础币数量组织订单，并额外留手续费储备，不能认为每格金额都恰好等于 `本金 / N`。

对每个候选格数，本程序按统一基础币数量逐格分配资金：

```text
e_i = min(第 i 个价格, 参考现价)，i = 0..N−1
F = Σ[e_i × (1 + f)]             # 每一单位 q 的建仓/买单资金
a = 手续费预留比例              # 默认 5%
I(q) = q × F / (1 − a)           # 包含预留的总投入
V(q) = I(q) − q × F              # 预留金额
```

低于现价的格子按对应买价留钱，高于或等于现价的格子按现价购买基础币，再在该格的上端卖出。手续费预留是本模型自己的安全余量，**不是复制币安内部算法**。在止损估算中把全部预留视作以现价买入的基础币，避免把它直接当成永远不会跌的 USDT。

最终必须同时满足：

- 最低价格那一笔的 `q × 下限 ≥ max(建议单笔金额, 交易所最低金额)`，默认建议金额为 30 USDT。
- 数量符合 stepSize、minQty、maxQty；最大价格的订单不超过 maxNotional。
- 区间和停止价满足价格边界，所有网格价位严格递增。
- 最差一格净收益达到门槛，投入和止损情景都不超预算。

经典算法从 150 格向下搜索，选**第一个满足全部条件**的方案。自适应比较不同 ATR 宽度与全部可行格数，另做滚动验证及最终检验。`--max-grids` 改搜索上限，`--grids` 固定格数；两种算法都不替换不合法固定值，若无法做网格就报错。

本程序只支持 2～170 格的静态模型。币安目前还支持更多格数的动态订单机制，但资金和活跃订单行为不同，因此这里不外推。交易所规则会变，具体支持数量以创建页面为准。

### 为什么投入增加，格数可能不变

**自适应选的是历史评分更高的可行格数，没有 5 格上限。** 当资金约束已满足时，增加投入主要放大每格数量；评分用本金收益百分比、回撤和成本压力计算，因此最优格数可能相同。经典算法则选择最多可行格数；还可以填「固定格数」/ `--grids N`，不可行时明确报错。

更多格数会缩小价差并把资金分到更多订单。总收益取决于每格数量、扣费价差、完整成交次数和库存浮盈亏，不与格数简单成正比。以区间 81993.21～88606.77、每边成本 0.15% 为例：5 格理论单格净收益约 1.2612%，12 格约 0.3476%，13 格约 0.2976%，后者低于默认 0.3% 门槛。这些是截图参数复算，不是未来收益预测。实际候选搜索、资金缩放复核见[本版验收记录](docs/grid-modes-verification.md)。

## 4. 为什么用账户风险来反推投入

先算可承受的策略情景亏损：

```text
B = 账户总资产 × 风险百分比
```

然后模拟从启动价一路下跌：启动买入的基础币被持有，下方买单依次成交，最终在 SL 卖出；不把尚未发生的网格利润算进安全垫。

结合上一节的 `I(q)` 和 `V(q)`：

```text
止损时基础币总量 Q = N × q + V(q) / [现价 × (1 + f)]
止损情景亏损 L(q) = I(q) − Q × SL × (1 − f)
要求 I(q) ≤ 可投入本金，L(q) ≤ B
```

`I(q)` 和 `L(q)` 对 q 都是线性的，所以可以直接反推允许的最大 q，再受数量及金额限制约束。数量向下取整；投入向上取整到 0.01 USDT；风险预算先留 0.01 USDT 的取整余量，最后复核。

默认 2% 是方便起步的**可调资金纪律**。CME 明确指出 2% 门槛本身具有任意性，不是科学证明的最优比例；真正可验证的是账户预算与头寸数量之间的计算关系。见 [CME 的 2% 风险规则说明](https://www.cmegroup.com/education/courses/trade-and-risk-management/the-2-percent-rule)。多个机器人同时运行时，预算还要合并考虑；这里的 2% 只约束这一个方案。

这也修正了原文两处误导：

- **固定 1:2 盈亏比不能保证正期望。** 期望还取决于实际胜率、成交、手续费和尾部亏损。网格包含多次交易及变化持仓，也不能套用一次买入的 1:2 来保证全策略盈利。
- **“账户亏损永远不超过 2%”不成立。** 上述约束是在假定成本和成交条件下成立。行情跌穿 SL、滑点更大、未完整成交或基础币没有卖出时，实际损失会更大。

报告额外显示 SL 以下再跌 5% 的压力情景，帮助看清这种差异。它仍不是任何行情下的最大亏损上界。也不会把原文的约 55 USDT 亏损、9.1% 本金回撤直接叫作“健康阈值”。

## 5. TP 为什么放在上限外

币安现货网格的 SL 要低于区间下限，TP 要高于上限。这里的 TP 更准确地说是**终止机器人价格**。

上涨穿过所有格子后，网格基础币通常已逐步卖出，仅可能剩手续费储备或少量残余；从上限继续涨到 TP 并不会让已卖掉的币继续升值。TP 的作用是结束旧箱体策略，方便重新评估。它不是必须冲到该价才能兑现区间内利润，也不是“开启主升浪”的证明。

ATR 模式的区间外缓冲跟随波动；百分比和手动模式默认用 3% 缓冲。若已有更可靠的结构失效点，可以手动覆盖停止价。不要为了把预计收益凑成 1:2 而把 TP 推到难以达到的位置。

## 6. 保存配置、导出和常用参数

修改 [examples/grid.toml](examples/grid.toml)，再运行：

```bash
cargo run --locked -- --config examples/grid.toml
```

TOML 字段用下划线，例如 `risk_pct`；CLI 用短横线，例如 `--risk-pct`。未知配置字段会报错，防止 `risk_ptc` 之类拼写错误悄悄套用默认值。配置中的相对 CSV 路径以该 TOML 所在目录为基准；CLI 的相对路径以当前工作目录为基准。

查看所有参数：

```bash
cargo run --locked -- --help
```

展开网格价格，或保存 JSON：

```bash
cargo run --locked -- --capital 600 --price 84000 --levels
cargo run --locked -- --capital 600 --price 84000 --json --output plan.json
```

JSON 的价格和每格数量是普通十进制字符串，方便精确显示及复制；风险金额和收益率是浮点估算。计算使用 f64，输出价格/数量再按步长用整数格式化；这不是可用于直接实盘下单的精确十进制交易引擎。超出安全精度范围的输入会拒绝。

| 常见参数 | 默认值 | 意义 |
| --- | --- | --- |
| `--equity` | 等于 capital | 账户总资产 |
| `--algorithm` | CLI classic / 桌面 adaptive | 经典最多格数 / 历史滚动验证选参；自适应需要 ATR 与足量 OHLC |
| `--grid-mode` | geometric | geometric 等比 / arithmetic 等差；与 `--mode` 区间来源独立 |
| `--risk-pct` | 2 | 单策略止损情景风险比例 |
| `--fee-pct` | 0.1 | 每边手续费百分比 |
| `--slippage-pct` | 0.05 | 每边额外成本百分比 |
| `--min-net-pct` | 0.3 | 最差一格净收益门槛百分比 |
| `--min-order-usdt` | 30 | 最低一笔网格订单建议金额 |
| `--reserve-pct` | 5 | 模型手续费预留比例 |
| `--max-grids` | 150 | 格数搜索上限，最大 170 |
| `--history-bars` | 历史获取 / 桌面为 180 | 最近已收盘 K 线根数，须大于 ATR 周期，最多 1000 |
| `--interval` / `--atr-period` | 1d / 14 | K 线周期 / Wilder ATR 周期 |
| `--proxy-url` | 不指定 | 公开行情请求使用的 HTTP / HTTPS / SOCKS5 代理 |
| `--tick-size` / `--step-size` | 0.01 / 0.00001 | **离线示例**价格/数量精度 |
| `--min-notional` / `--min-qty` | 10 / 0.00001 | **离线示例**最低金额/数量 |

离线值不代表任意币种当前规则，换交易对后尤其要核对。联网模式从 `/api/v3/exchangeInfo` 读取 PRICE_FILTER、LOT_SIZE、MIN_NOTIONAL/NOTIONAL 和 MAX_NUM_ORDERS，存在两个最低金额过滤器时取较严格者；参数仍要通过币安创建预览。每次发出三个公开 GET（规则、报价、服务器时间），ATR 模式、明确指定 `--history-bars` 或独立获取历史时再请求 K 线。桌面联网模式始终使用所选历史根数，方案 JSON 包含同批历史快照。程序不读账户持仓或手续费信息，不发送身份认证头。规则依据见[币安过滤器文档](https://developers.binance.com/en/docs/products/spot/filters)，数据格式见[公开行情端点文档](https://developers.binance.com/en/docs/catalog/core-trading-spot-trading/api/rest-api/market)。

动态 PERCENT_PRICE、MARKET_LOT_SIZE、账户/机器人权限、已有订单额度、币安实际基础币预留和市价清算规则不属于完整验证范围。流动性不足、单边趋势和频繁更新停止价也不是靠增加格数就能解决的。程序不会自动追踪止损或持续修改机器人。

## 7. 科学依据与证据边界

可直接验证的部分是等差/等比价位、双边成本、过滤规则、Wilder ATR 与账户预算的数学关系。经典默认的 ±10%、3/1.5 倍 ATR、0.3% 净收益门槛、5% 预留和 2% 风险比例是透明可调假设；自适应在预声明区间宽度与格数集合内按历史评分选参，其他门槛仍不声称最优。

币安公布的 AI 参数使用均值和标准差等历史指标构造区间，并不是原文所述的统一“ATR 一键公式”。本程序选择 ATR 是为了能解释、能复算，不等同于复制币安 AI，也不声称优于它。参见[币安 AI 参数生成说明](https://www.binance.com/en/support/faq/detail/76bd4effa3c4456c971a1c6835762742)。

V0.3 新增成本及库存回放、滚动发展验证、独立最终检验和旧策略/买入持有对照，结果见 [本版验证记录](docs/adaptive-verification.md)。经典算法仍不依据收益选参。尚未做账户手续费调查或真实机器人创建验证；OHLC 不证明限价排队、部分成交及实际滑点。不能把历史回放、几格净收益或 ATR 当成未来盈利保证。

## 8. 开发与验证

源码中的模块、结构体/类、枚举、字段、函数输入/返回值以及关键局部变量均有中文注释。阅读计算逻辑可按下面顺序；所有金额以 USDT 计，所有 `_pct` 配置填百分比数值（`2` 代表 `2%`）。

| 文件 | 阅读目的 |
| --- | --- |
| [src/config.rs](src/config.rs) | 了解输入字段、默认假设、覆盖顺序和非法输入校验 |
| [src/candles.rs](src/candles.rs) / [src/data.rs](src/data.rs) | 了解已收盘样本校验、Wilder ATR、公开 API 及代理 |
| [src/planner.rs](src/planner.rs) / [src/precision.rs](src/precision.rs) | 了解线位、格数搜索、资金/风险反推及交易步长取整 |
| [src/optimizer.rs](src/optimizer.rs) / [src/backtest.rs](src/backtest.rs) | 了解自适应候选、时间划分、扣费库存回放、旧策略对照与观望门槛 |
| [src/model.rs](src/model.rs) | 了解方案和历史 JSON 中每个字段的单位与含义 |
| [desktop/macos/FormState.swift](desktop/macos/FormState.swift) / [desktop/macos/Engine.swift](desktop/macos/Engine.swift) | 了解表单如何调用真实 Rust 程序，以及 JSON 和取消处理 |
| [desktop/macos/PlannerModel.swift](desktop/macos/PlannerModel.swift) / [desktop/macos/PlannerView.swift](desktop/macos/PlannerView.swift) | 了解主线程状态、请求过期保护、历史图表和结果展示 |
| [scripts/build-macos.sh](scripts/build-macos.sh) / [docs/github-build.md](docs/github-build.md) | 了解本地打包及 GitHub 自动生成可运行应用 |

`Cargo.lock` 是 Cargo 自动生成的依赖锁文件，应提交到 Git；第三方依赖和 `target` / `dist` 构建产物不手工加注释。修改项目源码后可以运行现有验收：

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --test cli
cargo build --locked --release
bash scripts/build-macos.sh
bash scripts/test-macos.sh
```

测试在实现前确定失败情景，使用真实 CLI，以及 Swift 表单 → 包内 Rust 子进程 → HTTP / JSON 的端到端验证，没有函数级单元测试。模拟 API 和代理使用确定性数据，仅存在于测试中；应用不会连接测试服务。覆盖历史根数、独立获取、未收盘排除、API 上限、断档、代理、JSON 导出和错误退出。受限沙箱需允许绑定测试用回环端口。GUI 的实际显示与自动化桥接测试分别记录，详情见 [docs/verification.md](docs/verification.md) 和 [docs/desktop-verification.md](docs/desktop-verification.md)。

原公式资料核对日期：2026-10-02；V0.3 开源算法资料核对日期：2026-10-04。平台功能、限制和费率会调整，以当前公开规则和创建预览为准。
