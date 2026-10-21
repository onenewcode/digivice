# 桌面灵动岛 (Desktop Island)

基于 Espressif **ESP32-C6**（微雪 Waveshare ESP32-C6-LCD-1.28 开发板）构建的纯 Rust、`#![no_std]` / `#![no_main]` 嵌入式固件工程与电脑守护服务。工程运行于 Embassy 异步并发运行时之上，实现圆屏分带显存渲染、IMU 急动度敲击与手势物理交互、USB-Serial-JTAG 电脑守护交互、BLE 离席安全感知及 Flash 容灾黑匣子。

## 💡 项目定位与核心作用及使用指南

**Desktop Island（桌面灵动岛）** 是为极客开发者、重度 PC 用户量身打造的**桌面级智能物理交互终端与电脑硬件守护外设**。

借助微雪 ESP32-C6-LCD-1.28 的圆形彩屏、6 轴 IMU、BLE 5 射频以及高精度 RTC，本项目在软硬件协同层面解决了多项日常开发与桌面办公的核心痛点，并提供了直观的物理与命令行交互体验：

### 1. 桌面物理级“灵动岛”：主机状态与 AI 任务实体化投射

- **解决痛点**：开发过程中，多显示器或全屏工作流常常遮挡系统关键监控与耗时任务。Desktop Island 常驻桌面边缘，实时以弧形仪表呈现 CPU 负载、GPU 占用、内存消耗与电池状态；当 AI Agent 或构建脚本运行时，以脉冲流光呈现深度思考（`Thinking`）、以呼吸环呈现等待人类确认（`WaitConfirm`）、并在构建完成或异常时弹出醒目通知。
- **🎮 如何使用**：
  1. **启动 PC 守护程序**（自动流送硬件监控与对齐时钟）：
     ```sh
     # 安装依赖：pip install pyserial psutil
     python3 tools/host_agent.py
     ```
     守护服务会自动检测 Type-C 端口，建立全双工通信，并以 2~5Hz 持续刷新桌面屏上的三联系统仪表。
  2. **在终端、CI 脚本或 AI Agent 流程中注入动态状态**：
     ```sh
     # 触发 AI / 耗时编译思考中脉冲
     python3 tools/host_agent.py --notify "Claude is thinking" --state THINKING --progress 40

     # 触发等待人工确认询问
     python3 tools/host_agent.py --notify "Approve rm -rf build/?" --state WAIT_CONFIRM

     # 任务成功或测试失败弹窗
     python3 tools/host_agent.py --notify "Cargo Test Passed" --state SUCCESS
     python3 tools/host_agent.py --notify "Compile Error" --state ERROR
     ```

### 2. 桌面级免按键物理交互：急动度敲击与空间姿态控制

- **解决痛点**：长时间沉浸于键盘编码或双屏浏览时，频繁伸手抓按键或寻找终端确认极为繁琐。
- **🎮 如何使用**：
  - **双击敲击桌面确认（Jerk Tap）**：当屏幕显示 `WaitConfirm` 胶囊时，**在桌面上直接用手指敲击两下**（约 0.3 秒间隔），IMU 探测到震动急动度（Jerk）峰值后立即向 PC 发送批准执行信号（终端会伴随响铃并继续执行任务）。
  - **物理倾斜切页（Tilt Navigation）**：轻轻将开发板**向左或向右微倾（约 18°）**，即可在【硬件自检仪表】、【灵动岛主视图】与【桌宠治愈视图】之间直观巡航（板载侧边 `BOOT` 键专用于芯片 ROM 烧录 Strapping，不作为业务功能键，全机交互实现 100% 纯物理姿态掌控）。
  - **朝下扣放免打扰（FaceDown DND）**：开会或休息时，直接将圆屏**面朝下扣在桌面上**，重力向量翻转触发即刻熄屏休眠并开启免打扰；随手翻起重新朝上即刻亮屏恢复。

### 3. 离席自动安全锁屏与在席守护（BLE Presence Watchdog）

- **解决痛点**：离开工位常忘记锁定电脑，导致代码与隐私暴露。
- **🎮 如何使用**：
  - 用户随身携带已配对的智能手机或 BLE 手环。
  - ESP32-C6 固件后台持续监听指定设备的广播 RSSI，并运用 EMA（指数滑动平均）平滑滤波。
  - 当用户起身离开工位（信号衰减并超时判决为 `AWAY`），固件向电脑发送事件，`tools/host_agent.py` 自动调用系统原生命令执行锁屏（macOS: `pmset displaysleepnow`，Linux: `loginctl lock-session`，Windows: `LockWorkStation`）；用户回到工位即刻唤醒亮屏。

### 4. 离线自主待机与工业级持久化容灾

- **解决痛点**：外设在主机断电或断连后容易黑屏瘫痪，发生死机时缺乏硬件排障证据。
- **🎮 如何使用**：
  - **无缝离线待机**：直接拔下 Type-C 线或电脑休眠，内置 10 秒看门狗超时后，设备自主维持低功耗走时，由板载 PCF85063 硬件 RTC 与纽扣电池自主精确走时；重新插上电脑后秒级恢复桌面灵动岛。
  - **导出黑匣子日志**：遇到异常掉电或系统断言异常后，重新插上电脑并运行：
    ```sh
    python3 tools/host_agent.py --dump-logs
    ```
    即可秒级从片内 Flash 导出最后保存的 64 字节灾难现场快照用于分析。

## 环境准备

1. 安装 Rust 工具链管理器（rustup）。
2. 安装目标芯片与组件（`rust-toolchain.toml` 会在进入目录时自动触发安装）：

   ```sh
   rustup toolchain install stable --component rust-src --target riscv32imac-unknown-none-elf
   ```

3. 安装烧录工具（提供 `espflash` CLI，用于 `cargo run` 自动烧录与监视）：

   ```sh
   cargo install espflash --locked
   ```

4. 硬件准备：一块微雪 ESP32-C6-LCD-1.28，用板载 Type-C 口连接电脑即可，
   不需要额外的调试探针。

## 构建与运行

```sh
# 编译（使用 debug profile，已开启体积优化）
cargo build

# 编译 release 版本
cargo build --release

# 编译并烧录到开发板，同时打开串口日志
cargo run
```

`cargo run` 的 runner 在 `.cargo/config.toml` 中配置为：

```sh
espflash flash --chip esp32c6 --monitor
```

如需列出串口、排查连接问题：

```sh
espflash list-ports
```

### 烧录与日志

本工程用 [`espflash`](https://github.com/esp-rs/espflash) 烧录，日志经
`esp-println` 输出到 USB-Serial-JTAG，可用 `espflash monitor` 查看：

```sh
cargo espflash flash --chip esp32c6
```

完整说明见 [docs/flashing.md](docs/flashing.md)。

### 硬件引脚与资源

板载 LCD、TF 卡、IMU、RTC 等外设的引脚占用见
[docs/hardware-esp32-c6-lcd-1.28.md](docs/hardware-esp32-c6-lcd-1.28.md)。

## 代码检查

```sh
cargo fmt --all -- --check
cargo clippy --all-features --workspace -- -D warnings
```

CI 会执行上述三类检查（见 `.github/workflows/rust_ci.yml`），使用的也是
`stable` 通道，与 `rust-toolchain.toml` 保持一致。

## 工具链说明（重要）

本工程在 `rust-toolchain.toml` 中固定 `channel = "stable"`。

- `build-std` 与 `-Z stack-protector=all` 属于**不稳定选项**，stable 通道默认
  拒绝 `-Z`。`.cargo/config.toml` 的 `[env]` 中已设置 `RUSTC_BOOTSTRAP = "1"`
  放行，因此直接 `cargo build` / `cargo run` 即可，无需额外环境变量。
- 这些依赖 `RUSTC_BOOTSTRAP` 的选项属于权宜之计。若要彻底摆脱它，需要移除
  `-Z` 相关的 rustflags 与 `build-std`；若改用 nightly，则可以去掉该
  `[env]` 项。

## 配置约定

- **编译相关设置**放在 `.cargo/config.toml`。
- **用户/运行时配置**放在 `.cargo/esp-config.toml` 的 `[env]` 段中；
  Espressif 系列 crate 的可用变量可通过 `esp-config` 查看与设置。
- 避免在源码中硬编码这些配置，统一走上述两个文件。

## 常见问题

- **烧录失败**：依次检查 `espflash list-ports` 是否能列出串口、串口权限
  （Linux 需要 `dialout` 组）、板子接线，以及 `.cargo/config.toml` 中的
  `--chip` 是否与目标一致；必要时按住 `BOOT` 进入下载模式。
- **分配到堆内存失败**：调整 `src/bin/main.rs` 中
  `esp_alloc::heap_allocator!` 的大小。注意 COEX 会额外占用内存。
- **`-Z` 选项报错**：确认 `.cargo/config.toml` 的 `[env] RUSTC_BOOTSTRAP = "1"`
  仍在（例如被 `[env]` 以外的来源覆盖时会失效），并确保使用的是
  `rust-toolchain.toml` 指定的 stable 通道。
- **`cannot parse the debug map ...` 报错**：这是目标目录中残留的旧构建产物
  导致的，切换工具链后清理 `target/` 重新构建即可，与配置无关。

## 参考资料

- esp-generate: <https://github.com/esp-rs/esp-generate>
- esp-config: <https://docs.espressif.com/projects/rust/esp-config/latest/esp_config/>
- esp-hal 文档: <https://docs.espressif.com/projects/rust/esp-hal/1.2.2/index.html>
- esp-hal 额外配置（esp32c6）: <https://docs.espressif.com/projects/rust/esp-hal/1.2.2/esp32c6/esp_hal/index.html#additional-configuration>
- esp-bootloader-esp-idf 额外配置: <https://docs.espressif.com/projects/rust/esp-bootloader-esp-idf/latest/esp32c6/esp_bootloader_esp_idf/index.html#additional-configuration>
- esp-rtos 额外配置: <https://docs.espressif.com/projects/rust/esp-rtos/latest/esp32c6/esp_rtos/index.html#additional-configuration>
- esp-radio 额外配置: <https://docs.espressif.com/projects/rust/esp-radio/latest/esp32c6/esp_radio/index.html#additional-configuration>
- esp-hal 示例: <https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.2.2/examples>
