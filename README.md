# 桌面数码暴龙机（Digivice）

[English](README_EN.md) | [中文](README.md)

> **An Autonomous Desktop Cyberpet & Digivice firmware targeting Espressif ESP32-C6 in pure `#![no_std]` Rust.**
> Features 6-stage authentic Bandai pixel evolution, Zero-FPU rotated sprite rasterization, and 6-DOF IMU tilt physics.

基于 Espressif **ESP32-C6** 和 Waveshare ESP32-C6-LCD-1.28 的纯 Rust、`#![no_std]` 桌面智能硬件。项目目标是一个**不需要电脑常驻程序也能自主工作的桌面数码宝贝**：自主计时、6轴姿态感知、数码兽进化节律、专注伴侣与断电状态持久化。

电脑或手机连接只是增强能力：

- USB-C：供电、刷写、JTAG 和调试日志；当前硬件的 USB-Serial-JTAG 不枚举 USB HID；
- BLE HID：让宠物直接控制电脑的播放/暂停、上下曲、音量和静音；
- BLE GATT MusicScene：接收音乐的节拍、能量和情绪特征，让宠物随音乐做动作；
- 连接断开后，宠物继续独立运行。

> 当前仓库采用“本地自主宠物”架构。总体拓扑和模块职责见 [`docs/architecture/README.md`](docs/architecture/README.md)。

## 核心体验与功能

### 经典万代像素与 6 阶段数码进化系统

重现初代数码暴龙机经典 16 色点阵视觉，完整实现亚古兽经典进化路线与双帧步行动画、数字矩阵进化特效：
- **幼年期 II (Baby II)**: 滚球兽 (Koromon, 16×16)
- **成长期 (Child)**: 亚古兽 (Agumon, 16×16)
- **成熟期 (Adult)**: 暴龙兽 (Greymon, 24×24)
- **完全体 (Perfect)**: 机械暴龙兽 (MetalGreymon, 32×32)
- **究极体 (Ultimate)**: 战斗暴龙兽 (WarGreymon, 32×32)
- **超究极体 (Super Ultimate)**: 奥米加兽 (Omegamon, 32×32)

### 6轴姿态感知与重力倾斜跟随（Pitch-Invariant Lateral Tilt）

- **仰角升起不偏转**：设备由平放拾起或调整仰角时（Pitch 变化），数码宝贝保持正直朝向，绝不因抬起产生左右歪斜；
- **侧倾精准跟随**：设备向左倾斜则数码兽向左偏转，向右倾斜则向右偏转，物理倾角与精灵旋转严密贴合（±45° 平滑旋转限制）；
- **摆锤摇晃训练**：左右周期性摇晃（Shake）触发数码机经典训练判定，累积胜率与力量值加速进化；
- **敲击与翻转休眠**：双击屏幕切换状态，屏幕朝下扣放（FaceDown）即刻进入静音深度睡眠。

### Zero-FPU 纯定点数实时仿射旋转光栅化

- 针对 ESP32-C6 RISC-V RV32IMAC 无硬件浮点单元架构，采用 91 点四分之一波定点正弦表（Q12 精度）计算反向仿射变换；
- 逐行屏幕带缓冲区（Screen Bands）流式光栅化与透明色键混合，避免占用整屏帧缓冲区与任何堆内存分配。

### 本地专注与桌面陪伴

设备通过 RTC、IMU、电池、充电、在席和手势运行本地 `FocusSession`：
- 专注模式、短休息、长休息与完成节律；
- 离席时自动休眠并降低背光，回到桌面时活泼苏醒；
- 独立持久化存储：基于 Flash 双扇区 Ping-Pong 架构，掉电安全保存数码兽阶段、训练次数与专注历史；
- 无需电脑、手机或云端常驻，离线 100% 完整运行。

### BLE 音乐联动与无线控制

- **BLE HID Consumer Control**：无需驱动，可控制电脑/手机的播放、暂停、上一曲、下一曲、音量与静音；
- **BLE GATT MusicScene**：接收宿主播放器的节拍（BPM）、能量与情绪，数码宝贝随音乐节拍蹦跳互动；
- 蓝牙断开或超时时自动平滑回退至自主日常行为。

## 电脑操作能力

现有 ESP32-C6 板载 USB 不是可编程 HID 控制器，因此电脑操作通过 BLE HID 完成。固件提供标准 HID Consumer Control 外设服务，只发送安全的媒体报告：

- 播放/暂停；
- 上一首/下一首；
- 音量增减；
- 静音；

不注入键盘文本、不移动鼠标、不执行系统锁屏。BLE 断开时清空过期瞬时动作，避免重连误触发。

## 架构文档

- [`docs/architecture/README.md`](docs/architecture/README.md)：全局拓扑和设计原则；
- [`docs/architecture/03-protocol-and-host.md`](docs/architecture/03-protocol-and-host.md)：BLE HID、MusicScene GATT 与 USB 调试边界；
- [`docs/architecture/04-ui-and-graphics.md`](docs/architecture/04-ui-and-graphics.md)：软萌宠物视觉、精灵资产和混合渲染；
- [`docs/architecture/05-radio-sensing.md`](docs/architecture/05-radio-sensing.md)：在席感知和无线业务；
- [`docs/architecture/06-concurrency-and-storage.md`](docs/architecture/06-concurrency-and-storage.md)：本地状态、异步任务和 Flash 双缓冲；
- [`docs/architecture/07-pet-and-interaction.md`](docs/architecture/07-pet-and-interaction.md)：PetBehavior 契约、默认宠物和快速替换方式。

## 硬件资源

| 模块 | 规格 |
| --- | --- |
| 主控 | ESP32-C6，RISC-V，最高 160MHz |
| 显示 | GC9A01，240x240 圆形 LCD |
| 传感器 | QMI8658 六轴 IMU |
| 时钟 | PCF85063 RTC |
| 无线 | BLE 5、Wi-Fi 6、IEEE 802.15.4（本项目首版使用 BLE） |
| 存储 | 16MB SPI NOR Flash |
| 供电 | USB-C、3.7V 锂电池、ETA6098 |

完整引脚和电气约束见 [`docs/hardware-esp32-c6-lcd-1.28.md`](docs/hardware-esp32-c6-lcd-1.28.md)。

## 环境准备

```sh
rustup toolchain install stable --component rust-src --target riscv32imac-unknown-none-elf
cargo install espflash --locked
```

## 构建与检查

```sh
cargo build
cargo build --release
cargo fmt --all -- --check
cargo clippy --all-features --workspace -- -D warnings
```

烧录和串口监视：

```sh
cargo run
espflash list-ports
```

具体说明见 [`docs/flashing.md`](docs/flashing.md)。
