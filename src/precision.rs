//! 十进制精度：价格/数量按交易所步长取整，输出字符串通过整数运算保留小数位。

use anyhow::{Context, Result, ensure};

/// 交易所十进制步长的双重表示：f64 用于计算，整数单位与小数位用于精确文本输出。
/// 例如 0.00001 存为 units=1、scale=5，避免用浮点格式化拼出多余尾数。
#[derive(Clone, Debug)]
pub struct Step {
    /// 原始十进制步长去掉小数点后的整数，例如 0.05 对应 5。
    units: u64,
    /// 十进制小数位数，例如 0.05 对应 2，用于恢复补零格式。
    scale: usize,
    /// 步长的浮点计算值，整数计数限制保证换算仍在安全范围内。
    value: f64,
}

/// 步长取整方向；资金数量向下保守收缩，区间外线位向外保留缓冲。
#[derive(Clone, Copy)]
pub enum Direction {
    /// 向下取整，不增加数量/资金；区间下界及 SL 也向下缓冲。
    Down,
    /// 向上取整，保留区间上界/TP 缓冲；投入金额向上取到分。
    Up,
}

impl Step {
    /// 输入：普通十进制步长字符串；返回：格式与步长信息，拒绝科学计数和零值。
    pub fn new(text: &str) -> Result<Self> {
        let parts: Vec<_> = text.split('.').collect(); // 按小数点拆分步长，拒绝空位、多小数点及科学计数法。
        ensure!(
            parts.len() <= 2
                && !parts[0].is_empty()
                && parts
                    .iter()
                    .all(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit())),
            "步长必须为正十进制数，例如 0.01，不能使用科学计数法"
        );
        let scale = parts.get(1).map_or(0, |s| s.len()); // 原始步长小数位数，固定输出位数由此决定。
        ensure!(scale <= 12, "步长最多支持 12 位小数");
        let units: u64 = parts.concat().parse().context("步长超出数值范围")?; // 去掉小数点的整数单位或整数步长总量，避免文本输出的浮点误差。
        let value: f64 = text.parse()?; // 步长的 f64 表示，供价格/数量除法计算。
        ensure!(
            units > 0 && units <= 9_000_000_000_000_000 && value <= 1e12,
            "步长必须大于 0 且在安全精度范围内"
        );
        Ok(Self {
            units,
            scale,
            value,
        })
    }

    /// 输入：待取整值和方向；返回：步长整数倍；极小误差先吸附到整数步长。
    pub fn quantize(&self, value: f64, direction: Direction) -> Result<f64> {
        let mut count = self.count(value)?; // 数值包含的步长个数；方向取整前先吸附极小浮点误差。
        if (count - count.round()).abs() <= 1e-7 {
            count = count.round();
        }
        // 数值包含的步长个数；方向取整前先吸附极小浮点误差。
        let count = match direction {
            Direction::Down => count.floor(),
            Direction::Up => count.ceil(),
        };
        Ok(count * self.value)
    }

    /// 输入：已按步长取整的值；返回：固定小数位字符串，用整数拼接避免输出浮点尾巴。
    pub fn text(&self, value: f64) -> Result<String> {
        let count = self.count(value)?.round() as u64; // 数值包含的步长个数；方向取整前先吸附极小浮点误差。
        let units = u128::from(count) * u128::from(self.units); // 去掉小数点的整数单位或整数步长总量，避免文本输出的浮点误差。
        if self.scale == 0 {
            return Ok(units.to_string());
        }
        let factor = 10_u128.pow(self.scale as u32); // 10 的小数位数次方，拆分整数部分与补零小数部分。
        Ok(format!(
            "{}.{:0width$}",
            units / factor,
            units % factor,
            width = self.scale
        ))
    }

    /// 输入：价格或数量；返回：未取整步长数，确保 f64 的整数精度可用。
    fn count(&self, value: f64) -> Result<f64> {
        let count = value / self.value; // 数值包含的步长个数；方向取整前先吸附极小浮点误差。
        ensure!(
            count.is_finite() && (0.0..=9_000_000_000_000_000.0).contains(&count),
            "数值/步长组合超出安全精度范围，请调整输入"
        );
        Ok(count)
    }
}
