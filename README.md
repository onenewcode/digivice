# 桌面灵动宠物（Desktop Island）

基于 Espressif **ESP32-C6** 和 Waveshare ESP32-C6-LCD-1.28 的纯 Rust、`#![no_std]` 桌面智能硬件。项目目标是一个**不需要电脑常驻程序也能工作的桌面宠物**：它自己计时、感知姿态、运行专注节律、渲染情绪和保存本地数据。

电脑或手机连接只是增强能力：

- USB-C：供电、刷写、JTAG 和调试日志；当前硬件的 USB-Serial-JTAG 不枚举 USB HID；
- BLE HID：让宠物直接控制电脑的播放/暂停、上下曲、音量和静音；
- BLE MIDI/GATT：接收音乐的节拍、能量和情绪特征，让宠物随音乐做动作；
- BLE GATT：按需修改配置、读取统计和导出黑匣子；
- 连接断开后，宠物继续独立运行。

> 当前仓库已经采用“本地自主宠物”架构。总体架构和验收边界见 [`docs/architecture/README.md`](docs/architecture/README.md)。`docs/plan/` 仅保留本地迁移草稿并被 Git 忽略，不属于产品运行时或历史接口。

## 产品体验

### 软萌宠物，而不是缩小的电脑仪表盘

日常主视图采用软萌玩偶视觉：圆润轮廓、柔和渐变、腮红、高光、呼吸、眨眼、注视、弹跳和短剧情动作。情绪包括平静、开心、专注、困倦、惊讶、害羞、睡眠和欢迎。

关键表情使用 Flash 精灵资产，眼睛、腮红和姿态使用参数化绘制，按屏幕带解码并支持脏区域刷新，避免当前整屏几何重绘带来的表现和资源问题。

### 本地专注与陪伴

设备通过 RTC、IMU、电池、充电、在席和手势运行本地 `FocusSession`：

- 专注、短休息、长休息和暂停/完成；
- 倾斜、敲击和 FaceDown 参与状态切换；
- 离席时自动安静和降亮度，回到桌面时欢迎；
- 无电脑、无手机、无云端仍然完整工作。

### BLE 音乐联动

设备不接收完整音频。手机或电脑继续播放音乐，并通过 BLE MIDI 或 GATT 发送 `bpm`、拍点、能量、情绪和段落信息：

- 慢歌：困倦、呼吸、轻微摇摆；
- 稳定节拍：跟拍摆动、眨眼和表情变化；
- 高能量或副歌：开心、兴奋和短剧情；
- 音乐断开或数据超时：自动回到本地宠物行为。

BLE MIDI/GATT 不会自动分析所有流媒体音频；若播放软件不提供节拍，需要在播放端按需运行音乐分析插件或工具，但这不是设备的常驻运行依赖。

## 电脑操作能力

现有 ESP32-C6 板载 USB 不是可编程 HID 控制器，因此电脑操作通过 BLE HID 完成。固件提供标准 HID Consumer Control 外设服务，只发送安全的媒体报告：

- 播放/暂停；
- 上一首/下一首；
- 音量增减；
- 静音；
- 专注开始/暂停/完成的可配置媒体动作。

不注入键盘文本、不移动鼠标、不执行系统锁屏。BLE 断开时清空过期瞬时动作，避免重连误触发。

## 架构文档

- [`docs/architecture/README.md`](docs/architecture/README.md)：全局拓扑和设计原则；
- [`docs/architecture/03-protocol-and-host.md`](docs/architecture/03-protocol-and-host.md)：BLE HID、BLE MIDI/GATT 与 USB 调试边界；
- [`docs/architecture/04-ui-and-graphics.md`](docs/architecture/04-ui-and-graphics.md)：软萌宠物视觉、精灵资产和混合渲染；
- [`docs/architecture/05-radio-sensing.md`](docs/architecture/05-radio-sensing.md)：在席感知、配置和无线业务；
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

## 迁移说明

旧的 `tools/host_agent.py`、`HostPacket`、电脑遥测和电脑锁屏链路已从固件和仓库删除。USB-C 仅保留供电、刷写、JTAG 与日志职责；电脑媒体控制使用 BLE HID，音乐输入使用 BLE GATT。宠物行为通过 `src/pet/` 的 `PetBehavior` 接口替换，Focus、音乐场景和媒体控制集中在 `src/experience/`，但不依赖 BLE 传输。
