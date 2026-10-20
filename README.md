# island

基于 `esp-generate` 生成的 ESP32-C6 嵌入式固件工程，使用 Rust 的 `no_std` /
`no_main` 模式编写，运行在 Embassy 异步运行时之上，并集成了 Wi-Fi、BLE 与
插座式网络协议栈。

## 项目概览

| 项目 | 值 |
| --- | --- |
| 目标芯片 | `esp32c6` |
| 目标硬件 | 微雪 ESP32-C6-LCD-1.28（16 MB Flash，240x240 LCD） |
| 目标三元组 | `riscv32imac-unknown-none-elf` |
| Rust 版本 | `stable`（见 `rust-toolchain.toml`） |
| 语言版本 | edition 2024 |
| 运行模式 | `#![no_std]` + `#![no_main]`，启用 `alloc` |
| 构建产物 | `src/bin/main.rs` -> 可执行文件 `island` |

## 功能特性

- **异步运行时**：`esp-rtos` + `embassy-executor`，通过 `#[esp_rtos::main]`
  启动入口任务。
- **无线能力**：`esp-radio` 同时启用 `wifi`、`ble`、`coex`，即 Wi-Fi 与
  BLE 共用同一射频（COEX）。
- **BLE 协议栈**：`bt-hci` 提供 HCI 控制器，`trouble-host` 提供主机协议栈，
  当前支持 1 条连接、1 条 L2CAP 通道。
- **网络协议栈**：`embassy-net` + `smoltcp`，默认启用 DHCPv4、TCP、UDP。
- **内存分配**：`esp-alloc` 提供全局堆，共两块：64 KiB 可回收 RAM（`reclaimed`）
  加 64 KiB 常规 RAM。
- **日志**：`esp-println` 通过 USB-Serial-JTAG 输出，可用 `espflash monitor` 查看。
- **栈保护**：`-Z stack-protector=all` 与 `force-frame-pointers` 已开启，
  便于排查栈溢出和获取回溯。

## 目录结构

```
.
├── .cargo/
│   ├── config.toml      # 目标、runner、rustflags、build-std、环境变量
│   └── esp-config.toml  # Espressif crate 的运行时配置（[env]）
├── .github/workflows/   # CI：build / fmt / clippy
├── docs/
│   ├── hardware-esp32-c6-lcd-1.28.md  # 目标硬件（微雪 ESP32-C6-LCD-1.28）引脚与资源
│   └── flashing.md      # espflash 烧录与日志
├── src/
│   ├── lib.rs           # 库入口（#![no_std]）
│   └── bin/main.rs      # 固件入口，包含外设初始化与主循环
├── build.rs             # 链接脚本与友好的链接期报错提示
├── Cargo.toml           # 依赖与编译 profile
└── rust-toolchain.toml  # 固定 stable 工具链、组件与目标
```

## 环境准备

1. 安装 Rust 工具链管理器（rustup）。
2. 安装目标芯片与组件（`rust-toolchain.toml` 会在进入目录时自动触发安装）：

   ```sh
   rustup toolchain install stable --component rust-src --target riscv32imac-unknown-none-elf
   ```

3. 安装烧录工具：

   ```sh
   cargo install cargo-espflash --locked
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

## 修改入口任务

主逻辑位于 `src/bin/main.rs` 的 `#[esp_rtos::main]` 函数：

1. 初始化 CPU 时钟与外设；
2. 注册堆分配器；
3. 启动 `esp-rtos` 调度器；
4. 初始化 Wi-Fi 控制器与 BLE 传输/主机栈；
5. 在 `loop` 中周期性输出日志。

`main.rs` 顶部的注释记录了本次生成所用的参数：

```
generator parameters: -o esp32c6 -o esp32c6-wroom-1 -o unstable-hal -o alloc -o embassy
                      -o ble-trouble -o wifi -o stack-smashing-protection -o probe-rs
                      -o ci -o agents -o vscode -o zed
```

修改与生成选项相关的代码前，请先确认这些参数的语义。注意其中
`-o probe-rs` 是生成时的初始选择，本工程之后已统一改用 `espflash`。

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
