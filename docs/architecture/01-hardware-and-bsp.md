# 01. 硬件外设与板级支持包（BSP）架构

板级支持包（BSP）位于 [`src/board/`](file:///Volumes/HIKSEMI/code/island/src/board/)，负责将 Waveshare ESP32-C6-LCD-1.28 硬件开发板的物理引脚与外设驱动封装为高级、类型安全、零冗余的 Rust 接口。

---

## 1. 硬件外设与管脚约束映射

| 外设模块 | 硬件芯片 / 接口 | 管脚分配 | 通信协议 / 驱动模式 | 架构设计考量 |
| :--- | :--- | :--- | :--- | :--- |
| **圆形液晶屏** | GC9A01 1.28" (240x240) | SCK: `GPIO11`<br>MOSI: `GPIO21`<br>CS: `GPIO10`<br>DC: `GPIO19`<br>RST: `GPIO20` | SPI2 (40MHz Blocking) | 独占硬件 SPI，CS 独立拉低，关闭 TF 卡 CS (`GPIO15`) 杜绝总线竞争 |
| **屏幕背光** | N-MOS 开关电路 | `GPIO7` | LEDC 低速通道 0 (20kHz PWM) | 8位占空比，实现 0..100% 平滑调光与 1.2Hz 正弦脉冲呼吸 |
| **6轴惯性测量** | QMI8658 6-DOF IMU | SCL: `GPIO8`<br>SDA: `GPIO18`<br>INT1: `GPIO4`<br>INT2: `GPIO5` | I2C0 (400kHz Fast Mode) | I2C 地址 `0x6B`，采用 12 字节连续突发读取以降低总线开销 |
| **硬件实时时钟** | PCF85063ATL RTC | SCL: `GPIO8`<br>SDA: `GPIO18`<br>INT: `GPIO6` | I2C0 (400kHz Fast Mode) | I2C 地址 `0x51`，与 IMU 共享 I2C0，支持硬件掉电走时与软件自适应回退 |
| **供电电压检测** | ETA6098 锂电管理 | ADC: `GPIO0`<br>BC_STATUS: `GPIO23` | ADC1 通道 0 (11dB 衰减) + 数字输入 | $R_1=200\text{k}\Omega, R_8=100\text{k}\Omega$ 分压电路，3倍增益换算为轨电压；GPIO23 检测充电状态 |
| **交互按键** | 侧边 BOOT 按键 | `GPIO9` | 数字输入 (上拉电阻) | 物理低电平有效，作为系统视图切换与多击交互键 |
| **主机通信** | 原生 USB-Serial-JTAG | D+: `GPIO13`<br>D-: `GPIO12` | 原生异步 USB CDC 虚拟串口 | 免外部串口芯片，供电、通信、日志、烧录一线双用 |
| **内部保护引脚** | 片内 Flash / Strapping | `GPIO24 ~ GPIO30` | 内部硬件保留 | **强类型编译期封印**，禁止在用户态进行任何复用配置 |

---

## 2. 内存受限下的分带显存架构（`BandDisplay`）

### 2.1 显存困境
GC9A01 屏幕为 $240 \times 240$ 分辨率，在 16 位真彩色 RGB565 格式下：
$$\text{全屏双缓冲显存} = 240 \times 240 \times 2 \text{ bytes} \times 2 = 230.4\text{ KB}$$
ESP32-C6 总 SRAM 仅约 512KB，在 Wi-Fi 6 协议栈、BLE 5 广播接收与 RTOS 任务并发运行时，为显示分配 115KB~230KB 的连续静态显存极易导致内存碎片化或溢出。

### 2.2 240x20 分带光栅化机制
在 [`src/board/display.rs`](file:///Volumes/HIKSEMI/code/island/src/board/display.rs) 中，我们设计了 **BandDisplay 水平条带渲染器**：
- 定义单一静态分带缓冲区：
  ```rust
  pub const BAND_HEIGHT: usize = 20;
  pub const NUM_BANDS: usize = 12; // 240 / 20 = 12 bands
  pub band_buffer: [u16; (DISPLAY_WIDTH as usize) * BAND_HEIGHT], // 仅 9.6 KB!
  ```
- 内存开销从 115.2 KB 大幅降低至 **9.6 KB**，节省了 **91.7%** 的显存！
- 封装 `BandTarget<'a>`，实现 `embedded-graphics` 的 `DrawTarget`：
  ```rust
  impl<'a> DrawTarget for BandTarget<'a> {
      type Color = Rgb565;
      type Error = core::convert::Infallible;

      fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error> {
          for Pixel(Point { x, y }, color) in pixels {
              if x >= 0 && x < 240 && y >= self.y_offset && y < self.y_offset + 20 {
                  let idx = ((y - self.y_offset) as usize) * 240 + (x as usize);
                  self.buffer[idx] = color.into_storage();
              }
          }
          Ok(())
      }
  }
  ```
- **渲染循环**：`render_full_frame` 顺序迭代 12 个条带，在光栅化阶段对每个图元进行 $Y$ 轴硬件裁剪；条带绘制完毕后立即通过 40MHz SPI 执行 `draw_buffer` 直传。
- **局部脏矩形加速**：对于仅有灵动岛胶囊（$Y \in [22, 66]$）变化的场景，调用 `render_partial(22, 66, ...)` 仅刷新其中的 3 个条带，渲染耗时仅为全屏的 $25\%$。

---

## 3. QMI8658 6-DOF 惯性传感器突发采样（`src/board/imu.rs`）

在 [`src/board/imu.rs`](file:///Volumes/HIKSEMI/code/island/src/board/imu.rs) 中实现 QMI8658 芯片的低延迟驱动：
1. **量程与采样率设置**：
   - 加速度计：$\pm 4\text{g}$，输出速率 250Hz，灵敏度为 $8192\text{ LSB/g}$。
   - 陀螺仪：$\pm 512\text{ dps}$，输出速率 250Hz，灵敏度为 $64\text{ LSB/dps}$。
   - 启用片上低通滤波器（LPF Mode 0），截止频率约为 ODR 的 10%，在硬件层平抑高频电气杂波。
2. **12 字节连读突发性能优化**：
   不分别发起 6 次独立读取，而是利用 `REG_CTRL1` 的自动地址递增特性（Auto-Increment），单次连续读取 12 字节：
   ```rust
   let mut buf = [0u8; 12];
   self.i2c.write_read(QMI8658_ADDR, &[REG_AX_L], &mut buf)?;
   ```
   单次突发传输仅产生一次 I2C 起始与停止位，大幅降低 I2C 总线占用时间，保证 250Hz 采样的确定性实时响应。
3. **高精度定点数单位换算**：
   - 加速度转换为毫重力（mg）：
     $$\text{ax\_mg} = \frac{\text{raw\_ax} \times 1000}{8192} = \frac{\text{raw\_ax} \times 125}{1024}$$
   - 陀螺仪转换为毫度每秒（mdps）：
     $$\text{gx\_mdps} = \frac{\text{raw\_gx} \times 1000}{64} = \frac{\text{raw\_gx} \times 125}{8}$$
   全程采用 32 位整数算术，无浮点开销且不损失精度。

---

## 4. PCF85063 硬件与软件时钟自适应回退（`src/board/rtc.rs`）

针对某些批量批次可能未贴片 RTC 芯片、或 RTC 纽扣电池耗尽的场景，在 [`src/board/rtc.rs`](file:///Volumes/HIKSEMI/code/island/src/board/rtc.rs) 实现了双轨时钟机制：
1. **硬件自检与探测**：
   启动时向地址 `0x51` 的 `REG_CONTROL_1` 寄存器尝试读取。若应答成功，清除停止位（STOP bit），标记为 `RtcStatus::HardwarePresent`；若无应答，则无缝切换到 `RtcStatus::FallbackSoftware`。
2. **纳秒级软件时钟插值**：
   软件时钟记录初始纪元秒数 `soft_epoch_offset_sec` 与校准时刻的 `soft_start_instant: Instant`。读取时通过 `Instant::elapsed().as_secs()` 累加并执行纪元解算，保持与硬件一致的外部 API。
3. **通用时钟解算算法**：
   对外暴露 `epoch_to_datetime(total_secs: u64) -> DateTime` 与 `datetime_to_epoch(dt: &DateTime) -> u64`，完整处理平年（365天）、闰年（366天）与大小月份换算。

---

## 5. 供电轨与按键监控（`src/board/power_key.rs`）

1. **分压采样与 ADC 标定**：
   ETA6098 充电电路的 BAT/VBUS 输入接入 $R_1=200\text{k}\Omega$ 与 $R_8=100\text{k}\Omega$ 的电阻分压网络。
   - 衰减配置：`Attenuation::_11dB`，量程约 3100mV。
   - 换算公式：
     $$V_{\text{rail}} = \frac{\text{raw} \times 3100 \times 3}{4095} = \frac{\text{raw} \times 9300}{4095}\text{ (mV)}$$
2. **充电状态读取**：
   ETA6098 的 `BC_STATUS` 状态脚接入 `GPIO23`（内置上拉）：
   - 低电平：充电中；
   - 高电平：充电完成或待机。

---

## 6. 芯片级引脚强类型封印模式（`src/board/reserved_pins.rs`）

在 ESP32-C6 / ESP32-C6-WROOM-1 模组中，`GPIO24 ~ GPIO30` 7个引脚在芯片封装内直连 SPI Flash / PSRAM 与硬件 Strapping 配置。任何代码若对这些引脚调用 `Output::new` 或变更电平，将直接引发 SPI Flash 总线故障并触发 CPU Panic。

### 6.1 传统“软限制”的致命缺陷
在工程生成代码中，常简单书写：
```rust
let _gpio24 = peripherals.GPIO24; // 软限制：仅防止未使用编译告警
```
这只是一种局部变量重命名，后续任何开发者若误删该行，即可在别处重新获取并初始化该引脚。

### 6.2 编译期仿射类型（Affine Types）封印设计
在 [`src/board/reserved_pins.rs`](file:///Volumes/HIKSEMI/code/island/src/board/reserved_pins.rs) 中构建所有权黑洞容器：
```rust
pub struct ReservedPins {
    _p24: GPIO24<'static>,
    _p25: GPIO25<'static>,
    _p26: GPIO26<'static>,
    _p27: GPIO27<'static>,
    _p28: GPIO28<'static>,
    _p29: GPIO29<'static>,
    _p30: GPIO30<'static>,
}

impl ReservedPins {
    pub(crate) const fn seal(
        p24: GPIO24<'static>, ..., p30: GPIO30<'static>,
    ) -> Self {
        Self { _p24: p24, ..., _p30: p30 }
    }
}
```
1. **强制交纳所有权**：在 `BoardPeripherals` 结构体中，`gpio24..=gpio30` 被定义为必需字段。
2. **初始化强制封印**：调用 `Board::init` 时，必须交出这 7 个引脚单例的所有权，并被封装进 `Board._reserved_pins`。
3. **零访问途径**：`ReservedPins` 不提供任何公有字段、不实现 `Deref`，引脚单例永久不可被二次借用或修改。
4. **编译期严格拦截**：若调用者试图在别处使用 `peripherals.GPIO24`，Rust 编译器将在编译阶段直接报错：`use of moved value`。
