//! 现货等量等比网格的 OHLC 情景回放；库存、成本、跳空和清仓全部计入权益。

use crate::candles::Candle;
use crate::model::Plan;
use anyhow::{Result, ensure};
use serde::Serialize;

/// 同一静态网格在两种 OHLC 路径及双倍成本下的保守汇总，不是成交保证。
#[derive(Clone, Debug, Serialize)]
pub struct BacktestMetrics {
    /// 普通成本下两条路径中较差的清仓后净利润，单位 USDT。
    pub net_profit_usdt: f64,
    /// 净利润除以可投入本金，包含未投入现金，单位百分比。
    pub net_return_pct: f64,
    /// 普通成本下两条路径中的最大权益回撤，单位百分比。
    pub max_drawdown_pct: f64,
    /// 两条路径中较少的网格卖出完成次数，清仓不算网格成交。
    pub completed_cycles: usize,
    /// 较差收益路径的建仓、逐格买卖及最终清仓比例成本，单位 USDT。
    pub trading_costs_usdt: f64,
    /// 双倍每边成本下两条路径中较差的本金收益百分比。
    pub stress_net_return_pct: f64,
    /// 双倍成本下两条路径中的最大权益回撤百分比。
    pub stress_max_drawdown_pct: f64,
    /// O-H-L-C 和 O-L-H-C 两种普通成本路径各自的本金收益百分比。
    pub path_returns_pct: [f64; 2],
    /// 任一情景是否触发 SL；跳空按更差的开盘价清仓。
    pub stop_triggered: bool,
    /// 任一情景是否触发区间外停止价 TP。
    pub take_triggered: bool,
}

/// 单条路径清仓后的结果，用于组合普通及成本压力情景。
struct Outcome {
    /// 清仓后净利润，包含初始购币和全部比例成本。
    profit: f64,
    /// 按可投入本金计算的收益百分比。
    return_pct: f64,
    /// 从初始本金及后续权益峰值计算的最大回撤百分比。
    drawdown: f64,
    /// 逐格卖出完成次数，不计终止清仓。
    cycles: usize,
    /// 全部模拟交易比例成本，单位 USDT。
    costs: f64,
    /// 是否因 SL 终止，本段不会自动重启。
    stop: bool,
    /// 是否因 TP 终止，本段不会自动重启。
    take: bool,
}

/// 现金和基础币账簿；每个槽位对应一个买价与相邻的卖价。
struct Book {
    /// N+1 个已按 tickSize 取整的价格点。
    prices: Vec<f64>,
    /// N 个槽位的持币状态，false 表示挂买单。
    held: Vec<bool>,
    /// 固定每格基础币数量，不因回放收益增加订单数量。
    quantity: f64,
    /// 按给定成本购买并持有至终止的费用储备币。
    reserve_base: f64,
    /// 策略内部可用于下方买单的现金，不能借用未分配资金。
    cash: f64,
    /// 策略之外的未分配本金，计入总权益但不用于下单。
    idle: f64,
    /// 每边手续费与滑点假设的比例。
    cost: f64,
    /// 建仓、网格买卖和清仓的比例成本累计。
    costs: f64,
    /// 网格卖出完成次数。
    cycles: usize,
}

impl Book {
    /// 输入：合法静态方案和每边成本比例；返回：从 USDT 开始的现金及初始库存账簿。
    fn new(p: &Plan, cost: f64) -> Result<Self> {
        let prices = p
            .grid_prices
            .iter()
            .map(|v| v.parse())
            .collect::<Result<Vec<f64>, _>>()?;
        let held: Vec<_> = prices[..p.grid_count]
            .iter()
            .map(|v| *v >= p.reference_price)
            .collect();
        let quantity = p.quantity_per_grid.parse::<f64>()?;
        let reserve_base = p.fee_reserve_usdt / (p.reference_price * (1.0 + cost));
        let base = held.iter().filter(|v| **v).count() as f64 * quantity + reserve_base;
        let initial = base * p.reference_price;
        let cash = p.investment_usdt - initial * (1.0 + cost);
        ensure!(cash >= -1e-8, "回放初始购币超过策略投入");
        Ok(Self {
            prices,
            held,
            quantity,
            reserve_base,
            cash: cash.max(0.0),
            idle: p.capital_limit_usdt - p.investment_usdt,
            cost,
            costs: initial * cost,
            cycles: 0,
        })
    }

    /// 输入：当前账簿；返回：全部网格持币加费用储备币的基础币数量。
    fn base(&self) -> f64 {
        self.held.iter().filter(|v| **v).count() as f64 * self.quantity + self.reserve_base
    }

    /// 输入：估值价格；返回：含未分配现金、库存及预估卖出成本的可清算总权益。
    fn equity(&self, price: f64) -> f64 {
        self.cash + self.idle + self.base() * price * (1.0 - self.cost)
    }

    /// 输入：槽位、方向和成交价；返回：无；全额限价成交，不允许负现金或裸卖。
    fn trade(&mut self, i: usize, buy: bool, price: f64) {
        let notional = self.quantity * price;
        if buy {
            let debit = notional * (1.0 + self.cost);
            if self.cash + 1e-8 < debit {
                return;
            }
            self.cash = (self.cash - debit).max(0.0);
        } else {
            self.cash += notional * (1.0 - self.cost);
            self.cycles += 1;
        }
        self.held[i] = buy;
        self.costs += notional * self.cost;
    }

    /// 输入：清仓成交价；返回：无；全部网格库存和储备币只收一次卖出成本。
    fn liquidate(&mut self, price: f64) {
        let notional = self.base() * price;
        self.cash += notional * (1.0 - self.cost);
        self.costs += notional * self.cost;
        self.held.fill(false);
        self.reserve_base = 0.0;
    }
}

/// 单条 OHLC 路径的停止状态与权益轨迹，不预测 K 线内真实成交先后。
struct Simulation {
    /// 实际模拟的现金和币量账簿。
    book: Book,
    /// 轨迹的上一个价格，用于检测穿越挂单。
    cursor: f64,
    /// 区间下方的止损价。
    stop: f64,
    /// 区间上方的停止价。
    take: f64,
    /// 未触发 SL/TP 时为 true，终止后现金保持不变。
    active: bool,
    /// 该轨迹是否触发止损。
    stopped: bool,
    /// 该轨迹是否触发停止价。
    taken: bool,
    /// 初始本金和后续清算权益中的最高值。
    peak: f64,
    /// 逐点累计的最大权益回撤百分比。
    drawdown: f64,
}

impl Simulation {
    /// 输入：静态方案及成本比例；返回：已记录建仓成本损失的初始模拟状态。
    fn new(p: &Plan, cost: f64) -> Result<Self> {
        let mut state = Self {
            book: Book::new(p, cost)?,
            cursor: p.reference_price,
            stop: p.stop_loss.parse()?,
            take: p.take_profit.parse()?,
            active: true,
            stopped: false,
            taken: false,
            peak: p.capital_limit_usdt,
            drawdown: 0.0,
        };
        state.mark(p.reference_price);
        Ok(state)
    }

    /// 输入：当前价格；返回：无；以可清仓权益更新峰值和回撤，库存浮亏不能忽略。
    fn mark(&mut self, price: f64) {
        let equity = self.book.equity(price);
        self.peak = self.peak.max(equity);
        self.drawdown = self.drawdown.max((self.peak - equity) / self.peak * 100.0);
    }

    /// 输入：新价格与是否为开盘跳空；返回：无；按价格顺序成交，停止后不自动重启。
    fn segment(&mut self, end: f64, gap: bool) {
        if !self.active {
            return;
        }
        let stop = end <= self.stop;
        let take = end >= self.take;
        let target = if stop {
            self.stop
        } else if take {
            self.take
        } else {
            end
        };
        let down = target < self.cursor;
        let indices: Vec<_> = if down {
            (0..self.book.held.len()).rev().collect()
        } else {
            (0..self.book.held.len()).collect()
        };
        for i in indices {
            let price = self.book.prices[if down { i } else { i + 1 }];
            let crossed = if down {
                !self.book.held[i] && self.cursor > price && target <= price
            } else {
                self.book.held[i] && self.cursor < price && target >= price
            };
            if crossed {
                self.book.trade(i, down, price);
                self.mark(price);
            }
        }
        // 跳空下跌不保证按 SL 卖出；上跳的 TP 清仓仍用触发价作保守估算。
        let exit = if stop && gap {
            end.min(self.stop)
        } else {
            target
        };
        self.mark(exit);
        self.cursor = exit;
        if stop || take {
            self.book.liquidate(exit);
            self.active = false;
            self.stopped = stop;
            self.taken = take;
        }
    }
}

/// 输入：方案、后续 K 线、成本与路径顺序；返回：该路径清仓结果，不读取训练段以后的指标。
fn run_path(p: &Plan, candles: &[Candle], cost: f64, high_first: bool) -> Result<Outcome> {
    let mut state = Simulation::new(p, cost)?;
    for c in candles {
        state.segment(c.open, true);
        let path = if high_first {
            [c.high, c.low, c.close]
        } else {
            [c.low, c.high, c.close]
        };
        for price in path {
            state.segment(price, false);
        }
    }
    if state.active {
        state.book.liquidate(state.cursor);
        state.mark(state.cursor);
    }
    let profit = state.book.equity(state.cursor) - p.capital_limit_usdt;
    Ok(Outcome {
        profit,
        return_pct: profit / p.capital_limit_usdt * 100.0,
        drawdown: state.drawdown,
        cycles: state.book.cycles,
        costs: state.book.costs,
        stop: state.stopped,
        take: state.taken,
    })
}

/// 输入：合法方案和连续后续 K 线；返回：两条 OHLC 路径与双倍成本下的保守历史指标。
pub fn evaluate(p: &Plan, candles: &[Candle]) -> Result<BacktestMetrics> {
    ensure!(!candles.is_empty(), "历史回放需要后续 K 线");
    let cost = p.effective_cost_per_side_pct / 100.0;
    ensure!(cost * 2.0 < 1.0, "自适应双倍成本必须小于 100%");
    let normal = [
        run_path(p, candles, cost, true)?,
        run_path(p, candles, cost, false)?,
    ];
    let stress = [
        run_path(p, candles, cost * 2.0, true)?,
        run_path(p, candles, cost * 2.0, false)?,
    ];
    let worst = if normal[0].profit <= normal[1].profit {
        &normal[0]
    } else {
        &normal[1]
    };
    Ok(BacktestMetrics {
        net_profit_usdt: worst.profit,
        net_return_pct: worst.return_pct,
        max_drawdown_pct: normal[0].drawdown.max(normal[1].drawdown),
        completed_cycles: normal[0].cycles.min(normal[1].cycles),
        trading_costs_usdt: worst.costs,
        stress_net_return_pct: stress[0].return_pct.min(stress[1].return_pct),
        stress_max_drawdown_pct: stress[0].drawdown.max(stress[1].drawdown),
        path_returns_pct: [normal[0].return_pct, normal[1].return_pct],
        stop_triggered: normal.iter().chain(&stress).any(|v| v.stop),
        take_triggered: normal.iter().chain(&stress).any(|v| v.take),
    })
}

/// 输入：同额投入方案和期末价格；返回：同额买入持有、含双边成本及闲置现金的本金收益百分比。
pub fn buy_hold_return(p: &Plan, last: f64) -> f64 {
    let cost = p.effective_cost_per_side_pct / 100.0;
    let ratio = last * (1.0 - cost) / (p.reference_price * (1.0 + cost));
    p.investment_usdt * (ratio - 1.0) / p.capital_limit_usdt * 100.0
}
