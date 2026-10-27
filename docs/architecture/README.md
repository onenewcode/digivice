# ESP32-C6 自主桌面宠物系统架构总览

本工程面向 Waveshare **ESP32-C6-LCD-1.28**，目标是构建一个**脱离外部连接也能成立的桌面宠物**：设备自己计时、感知、渲染、陪伴和生长；手机或电脑只在需要时通过 BLE 发送音乐特征或接收 BLE HID 外设事件。

USB-C 口在当前硬件上是固定功能的 USB-Serial-JTAG 控制器，负责供电、刷写、JTAG 和调试日志，**不是可编程 USB HID 控制器**。电脑操作能力由固件内置的 BLE HID 服务提供，不需要任何宿主程序。

---

## 1. 设计原则与系统拓扑

### 1.1 产品原则

- **设备自洽**：无 USB 数据连接、无 BLE 连接、无云端时，宠物仍能完成启动、计时、进化、互动、睡眠和唤醒。
- **连接是增强而非依赖**：BLE 用于 HID、音乐特征和在席感知；断开后自动回退本地状态机。
- **本地优先**：RTC、IMU、电量、在席状态和本地 Flash 是运行时真源，不接受外部遥测覆盖核心状态。
- **可爱优先**：软萌玩偶视觉和有时间轴的动画是主界面，仪表盘只作为自检或诊断视图。
- **固定资源**：所有通信、动画和资产解码均使用有界队列与固定容量，保持 `no_std` 和零大栈帧约束。

### 1.2 目标拓扑

```mermaid
flowchart TD
    subgraph Hardware["ESP32-C6 桌面宠物硬件"]
        LCD["GC9A01 240x240 圆屏"]
        IMU["QMI8658 六轴 IMU"]
        RTC["PCF85063 RTC"]
        POWER["电池/充电/背光"]
        FLASH["16MB SPI NOR Flash"]
        RADIO["BLE 5 射频"]
        USB["USB-Serial-JTAG"]
    end

    subgraph Local["设备本地运行时"]
        SENSOR["sensor_task\n姿态/手势/电量"]
        PET["PetBehavior\n可替换宠物实现"]
        MUSIC["MusicScene\n节拍/能量/情绪"]
        APP["AppStateMachine\n本地状态协调"]
        RENDER["ui_render_task\n圆屏/背光"]
        STORAGE["storage_task / StorageManager\n启动加载/限频保存/双扇区"]
        HID["BLE HID\n媒体控制"]
        GATT["BLE GATT\nMusicScene"]
    end

    subgraph External["可选外部设备"]
        PHONE["手机/电脑\n音乐分析"]
        PC["电脑操作系统\nBLE HID 配对"]
    end

    IMU --> SENSOR
    POWER --> SENSOR
    RTC --> APP
    SENSOR --> APP
    RADIO --> APP
    GATT --> APP
    APP --> HID
    SENSOR --> PET
    MUSIC --> PET
    PET --> APP
    APP -->|保存请求/确认| STORAGE
    APP --> RENDER
    RENDER --> LCD
    RADIO --> GATT
    PHONE -. "BLE GATT MusicScene" .-> MUSIC
    HID -. "标准媒体控制" .-> PC
    USB -. "供电/刷写/JTAG/日志" .-> PC
    FLASH --> STORAGE
```

---

## 2. 源码目录与多 Crate 架构职责

本工程遵循工业级嵌入式 Rust 标准，采用 **Cargo Workspace（多 Crate 工作区）** 架构，彻底解耦纯业务领域模型、硬件外设抽象与系统固件并发装配：

```text
digivice/
├── docs/architecture/          # 与实现严格同步的架构规范
├── partitions.csv              # ESP32-C6 16MB Flash 分区表（应用 4MB / 自定义存储 12MB 硬分界线）
├── espflash.toml               # 固件刷写与分区表绑定配置
├── scripts/                    # 自动化布局校验 (check-flash-layout.py) 与宿主测试启动脚本 (test-host.sh)
├── crates/
│   ├── digivice-core/          # 【纯逻辑核心库】(#![no_std]，零硬件依赖，跨平台原生运行 cargo test)
│   │   ├── src/
│   │   │   ├── domain/         # 核心跨层抽象领域模型 (ViewMode, DeviceOrientation, MusicScene, HidAction)
│   │   │   ├── drivers/        # 平台无关的通用传感器芯片驱动 (QMI8658 IMU, PCF85063 RTC)
│   │   │   ├── gesture/        # 姿态滤波 (Attitude) 与敲击/倾斜手势识别引擎
│   │   │   ├── pet/            # PetBehavior 接口、滚球兽自主状态机、模块化资产 (assets/) 与弹性物理
│   │   │   ├── protocol/       # BLE HID Consumer 报告编码、在席追踪 (presence) 与通信协议模型
│   │   │   ├── storage/        # 可靠存储：双扇区配置 (config)、黑匣子 (blackbox)、容灾管理 (manager)、CRC32
│   │   │   └── ui/             # 纯数据展示模型 (UiState, DirtyBounds)
│   │   └── tests/              # 5 大高内聚集成测试套件 (storage, gesture, pet_lifecycle, clock, protocol)
│   ├── digivice-bsp/           # 【板级支持包】(ESP32-C6 硬件外设与物理引脚隔离)
│   │   └── src/
│   │       ├── display.rs      # GC9A01 240x240 圆形 LCD 驱动（分带光栅化与 LEDC 背光 PWM）
│   │       ├── power_key.rs    # 电池分压采样 ADC、充电状态 GPIO 与物理按键消抖
│   │       └── lib.rs          # 板级总线单例、引脚绑定 (BoardPeripherals) 与安全隔离
│   └── digivice-firmware/      # 【系统固件主程序】(Embassy 并发运行时与主入口)
│       └── src/
│           ├── bin/
│           │   ├── digivice.rs # 正统桌面数码兽产品组合根：初始化 CPU 时钟、堆、板卡、BLE栈并启动任务
│           │   └── selftest.rs # 独立工厂硬件自检诊断工具：全外设检验与实时遥测视窗
│           ├── app/            # 本地应用状态协调与工作流 (AppStateMachine, ConfigState)
│           ├── ipc/            # 全局静态 Channel 与 Signal 权威异步总线
│           ├── radio/          # 完整 BLE 射频：Trouble-host GATT 服务与 esp-radio 射频驱动
│           ├── tasks/          # 异步并发任务汇集层 (sensor, rtc, radio, ui, app, storage)
│           └── ui/             # 基于 embedded-graphics 的圆屏像素呈现层 (pet_view, status_view, selftest_view)
```

连接适配器只向本地类型化通道投递有界事件；USB-Serial-JTAG 不属于应用业务通道。

---

## 3. 分模块架构索引

1. **[01. 硬件外设与板级支持包](./01-hardware-and-bsp.md)**
   - 板载资源、引脚隔离、带缓冲显示和低功耗约束。
2. **[02. 物理姿态解算与手势识别](./02-gestures-and-physics.md)**
   - 重力滤波、敲击、倾斜、扣放和取消安全性。
3. **[03. BLE/USB 外设边界](./03-protocol-and-host.md)**
   - BLE HID、MusicScene GATT、BLE 在席和 USB-Serial-JTAG 的边界。
4. **[04. 宠物视觉与混合渲染](./04-ui-and-graphics.md)**
   - 软萌玩偶美术、精灵资产、关键帧、图层和脏区域刷新。
5. **[05. BLE 在席、HID 与音乐联动](./05-radio-sensing.md)**
   - 在席感知、HID 外设和 MusicScene 特征输入。
6. **[06. 并发、状态与可靠存储](./06-concurrency-and-storage.md)**
   - 本地状态时钟、固定容量队列和 Flash 双缓冲。
7. **[07. 可替换宠物与交互扩展](./07-pet-and-interaction.md)**
   - PetBehavior/PetEvent 契约、自定义宠物接入和交互层边界。

---

## 4. 强制架构同步边界

涉及以下任一项时，必须同时更新本目录中相应文档和路线图：

- BLE 服务、HID 报告、音乐特征格式或 USB 调试边界；
- 宠物视图、动画状态、精灵资产格式或刷新策略；
- 任务拓扑、静态通道或 Flash 存储布局；
- 板级引脚、显示带缓冲、BLE 在席阈值和功耗策略。


