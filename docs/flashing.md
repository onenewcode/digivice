# 烧录与日志

本工程使用 [`espflash`](https://github.com/esp-rs/espflash) 烧录，通过板载
Type-C 口（芯片原生 USB-Serial-JTAG），不需要额外的调试探针。

硬件引脚和 Type-C 连接说明见
[hardware-esp32-c6-lcd-1.28.md](hardware-esp32-c6-lcd-1.28.md)。

## 安装

```sh
cargo install espflash --locked          # 命令行工具，烧录已有 ELF
cargo install cargo-espflash --locked    # cargo 子命令，编译 + 烧录一条命令
```

## 烧录目标 Binary（双 Bin 架构）

本项目遵循严格的生产固件与工厂测试治具解耦原则，提供两个独立可执行目标：

| 目标名称 | 源码入口 | 职责定位 | 烧录命令 |
| :--- | :--- | :--- | :--- |
| **`digivice`** | `src/bin/digivice.rs` | **正统桌面数码兽产品固件**（完全剥离测试代码，专注纯粹桌宠养成） | `cargo espflash flash --bin digivice --monitor` |
| **`selftest`** | `src/bin/selftest.rs` | **工厂硬件自检测试诊断工具**（独立运行，检验屏幕、IMU、RTC、ADC与按键） | `cargo espflash flash --bin selftest --monitor` |

```sh
# 1. 烧录数码机主固件（默认推荐）
cargo espflash flash --bin digivice --monitor

# 2. 烧录出厂硬件自检测试固件
cargo espflash flash --bin selftest --monitor

# 烧录 release 优化版本
cargo espflash flash --release --bin digivice --monitor
```

设备端口一般会自动识别，需要时用 `-p /dev/ttyACM0` 指定。

也可以直接用 cargo runner（`.cargo/config.toml` 已配置）：

```sh
cargo run
```

## 查看日志

本工程用 `esp-println` 把日志输出到 USB-Serial-JTAG，`espflash` 的串口监视器
即可读取：

```sh
# 烧录后直接监视
cargo espflash flash --monitor

# 或单独打开监视器（不重新烧录）
espflash monitor --chip esp32c6
```

按 `Ctrl+R` 可以复位芯片。

## 常见参数

```sh
-p, --port <PORT>        指定串口
-B, --baud 921600        提高烧录波特率
-M, --monitor            烧录后打开串口监视器
-r, --monitor-baud <N>   监视器波特率（默认 115200）
-s, --flash-size 16mb    指定 Flash 容量（本板为 16 MB）
```

`--flash-size` 只影响镜像头里声明的容量。连接设备时 `espflash` 会自动探测
Flash 大小（探测失败才回退到默认 4 MB），所以通常不写也没问题。

## 其他子命令

```sh
espflash board-info                        # 读取芯片型号、MAC、Flash 等信息
espflash list-ports                        # 列出串口
espflash erase-flash                       # 整片擦除
espflash save-image --merge <ELF> out.bin  # 导出合并镜像
```

bootloader 和分区表不用自己准备：本工程不依赖 `esp-idf-sys`，`espflash` 会使用
其内置的默认 bootloader 和分区表。

## 进入下载模式

如果烧录时卡在等待设备，按住 `BOOT` 键再上电或复位，即可进入下载模式。

## 配置文件

两个可选配置文件：

- `espflash.toml`：项目级配置，可提交到仓库。
- `espflash_ports.toml`：串口配置，因人而异，**不要提交**。

项目级示例：

```toml
baudrate = 921600

[flash]
size = "16MB"
```

串口示例：

```toml
[connection]
serial = "/dev/ttyUSB0"
```

全局配置位置：

| 平台 | 路径 |
| --- | --- |
| Linux | `$HOME/.config/espflash/` |
| macOS | `$HOME/Library/Application Support/rs.esp.espflash/` |
| Windows | `%APPDATA%\esp\espflash\` |

环境变量 `ESPFLASH_PORT`、`ESPFLASH_BAUD`、`MONITOR_BAUD` 的优先级高于配置文件。

## 常见问题

- **报无串口**：用 `espflash list-ports --list-all-ports` 确认端口存在。
- **Linux 权限不足**：把用户加入 `dialout` 组，例如
  `sudo usermod -a -G dialout $USER`，重新登录或 `su - $USER` 生效。
- **看不到日志**：确认监视器已打开（`--monitor`），并注意端口不要被其他
  串口工具占用。
