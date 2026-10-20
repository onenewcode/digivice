# ESP32-C6 桌面灵动岛与态势感知雷达系统架构总览

本工程是基于 Espressif **ESP32-C6**（微雪 Waveshare ESP32-C6-LCD-1.28 开发板）构建的纯 Rust、`#![no_std]`、异步高可靠性桌面级智能交互终端固件与电脑守护服务。

---

## 1. 架构设计哲学与系统拓扑

整个系统采用基于 **Embassy 异步并发运行时** 的多任务协作式架构。硬件外设层、板级支持包（BSP）、算法过滤层、中央应用状态机与 UI 渲染管线各司其职，通过类型安全的静态 `Channel` 与 `Signal` 解耦。

```mermaid
flowchart TD
    subgraph Hardware["硬件设备层 (Waveshare ESP32-C6-LCD-1.28)"]
        GC9A01["GC9A01 1.28寸圆屏 (SPI2)"]
        QMI8658["QMI8658 6轴IMU (I2C0)"]
        PCF85063["PCF85063 RTC (I2C0)"]
        LEDC_PWM["背光PWM (GPIO7 / LEDC)"]
        ADC_KEY["供电ADC (GPIO0) / BOOT按键 (GPIO9)"]
        USB_JTAG["原生 USB-Serial-JTAG (GPIO12/13)"]
        RADIO_PHY["2.4GHz Wi-Fi 6 + BLE 5 (esp-radio)"]
        FLASH_CHIP["16MB SPI NOR Flash (片内SPI)"]
    end

    subgraph BSP["板级支持包 (src/board/)"]
        Display["BandDisplay (分带显存 9.6KB)"]
        Backlight["Backlight (LEDC 呼吸控制)"]
        ImuDriver["Qmi8658 (12B 突发采样)"]
        RtcDriver["Pcf85063 (RTC硬件/软件后备时钟)"]
        PowerKey["PowerKeyMonitor (分压检测+按键消抖)"]
        ReservedPins["ReservedPins (GPIO24..30类型级封印)"]
    end

    subgraph Engine["数据处理与算法层"]
        ImuFilter["ImuFilter (低通重力分离+死区航向积分)"]
        GestureDetector["GestureDetector (Jerk敲击/左右倾斜/朝下扣放)"]
        ButtonDetector["ButtonDetector (短按/双击/长按状态机)"]
        WifiRadar["WifiRadarAnalyzer (邻道频谱泄漏重叠模型)"]
        BlePresence["BlePresenceTracker (EMA滤波+离开滞后比较)"]
        StorageMgr["StorageManager (Flash分区配置+循环黑匣子)"]
    end

    subgraph AsyncBus["Embassy 异步通信总线 (src/bin/main.rs)"]
        IMU_SIGNAL["IMU_SIGNAL"]
        POWER_SIGNAL["POWER_SIGNAL"]
        TIME_SIGNAL["TIME_SIGNAL"]
        TIME_SYNC_SIGNAL["TIME_SYNC_SIGNAL"]
        UI_STATE_SIGNAL["UI_STATE_SIGNAL"]
        GESTURE_CHANNEL["GESTURE_CHANNEL (16)"]
        HOST_PACKET_CHANNEL["HOST_PACKET_CHANNEL (8)"]
        DEVICE_EVENT_CHANNEL["DEVICE_EVENT_CHANNEL (8)"]
        RADIO_REPORT_CHANNEL["RADIO_REPORT_CHANNEL (4)"]
    end

    subgraph Tasks["Embassy 并发协程任务集"]
        T_Sensor["sensor_task (250Hz IMU+按键+供电)"]
        T_USB["usb_task (串口通信与心跳看门狗)"]
        T_RTC["rtc_power_task (RTC同步与时钟分发)"]
        T_UI["ui_render_task (33Hz分带流水线渲染)"]
        T_App["app_state_task (中央单消费者状态总控)"]
    end

    subgraph HostPC["电脑端 (Host PC)"]
        HostAgent["tools/host_agent.py (跨平台Python守护进程)"]
        HostAction["AI Agent / 编译状态推送 / 离席自动锁屏"]
    end

    %% Hardware to BSP
    GC9A01 --> Display
    LEDC_PWM --> Backlight
    QMI8658 --> ImuDriver
    PCF85063 --> RtcDriver
    ADC_KEY --> PowerKey

    %% BSP to Tasks
    ImuDriver --> T_Sensor
    PowerKey --> T_Sensor
    Display --> T_UI
    Backlight --> T_UI
    RtcDriver --> T_RTC

    %% Sensor processing
    T_Sensor --> ImuFilter --> GestureDetector --> GESTURE_CHANNEL
    T_Sensor --> ButtonDetector --> GESTURE_CHANNEL
    T_Sensor --> IMU_SIGNAL
    T_Sensor --> POWER_SIGNAL

    %% Host Communications
    USB_JTAG <--> T_USB
    T_USB --> HOST_PACKET_CHANNEL
    DEVICE_EVENT_CHANNEL --> T_USB
    T_USB <--> HostAgent <--> HostAction

    %% Coordinator
    GESTURE_CHANNEL --> T_App
    HOST_PACKET_CHANNEL --> T_App
    RADIO_REPORT_CHANNEL --> T_App
    POWER_SIGNAL --> T_App
    TIME_SIGNAL --> T_App

    T_App --> TIME_SYNC_SIGNAL --> T_RTC
    T_App --> DEVICE_EVENT_CHANNEL
    T_App --> UI_STATE_SIGNAL --> T_UI
```

---

## 2. 完整源码目录结构与职责划分

```text
island/
├── .cargo/
│   └── config.toml             # 目标架构 riscv32imac-unknown-none-elf、runner 与 espflash 配置
├── Cargo.toml                  # 依赖配置（esp-hal、esp-rtos、embedded-graphics、trouble-host等）
├── docs/                       # 项目文档库
│   ├── architecture/           # [分模块架构文档专区]
│   │   ├── README.md           # [本文件] 架构总览与导航索引
│   │   ├── 01-hardware-and-bsp.md      # 板级外设、分带显存与强类型引脚封印
│   │   ├── 02-gestures-and-physics.md  # 姿态滤波、Jerk 敲击与手势状态机
│   │   ├── 03-protocol-and-host.md     # USB 双向帧协议、零堆序列化与 Python 守护服务
│   │   ├── 04-ui-and-graphics.md       # 圆屏 UI 引擎、调色板与 4 大视图渲染
│   │   ├── 05-radio-sensing.md         # 2.4GHz 空间频谱重叠模型与 BLE 离席感知
│   │   └── 06-concurrency-and-storage.md # 5 大协程拓扑、Actor 单消费者与 Flash 黑匣子
│   ├── flashing.md             # 固件烧录与环境依赖说明
│   ├── hardware-esp32-c6-lcd-1.28.md   # 硬件管脚映射、供电与时钟约束规格
│   └── plan/                   # 各阶段里程碑开发计划与设计规约
├── src/
│   ├── lib.rs                  # 库入口，声明全局 #![deny(warnings)] 与子模块
│   ├── bin/
│   │   └── main.rs             # 固件入口：全局硬件装配、静态总线与 5 大协程任务生成
│   ├── board/                  # 板级支持包 (BSP)
│   │   ├── mod.rs              # BoardPeripherals/Board 结构装配与 I2C0 互斥总线共享
│   │   ├── display.rs          # GC9A01 驱动与 240x20 分带缓冲区 (9.6KB RAM) 渲染引擎
│   │   ├── imu.rs              # QMI8658 6轴IMU 寄存器配置与 12 字节连续突发读取驱动
│   │   ├── power_key.rs        # ADC1 GPIO0 供电轨采样、ETA6098 充电检测与 BOOT 键读取
│   │   ├── reserved_pins.rs    # 硬件自举/内置Flash引脚（GPIO24..30）编译期零访问所有权封印
│   │   └── rtc.rs              # PCF85063 硬件 RTC 与软件自动 fallback 纳秒级后备时钟
│   ├── gesture/                # 物理交互与姿态融合
│   │   ├── mod.rs              # GestureEvent 事件枚举定义
│   │   ├── filter.rs           # 姿态解算器：一阶低通滤波分离重力向量与死区航向积分
│   │   └── detector.rs         # Jerk急动度敲击检测、倾斜死区锁定、朝下翻转与按键多击消抖
│   ├── proto/                  # USB 双向通信协议
│   │   ├── mod.rs              # 协议行解析器（SYS/ACT/SYNC/DUMP_LOGS）与 DeviceEvent Display 格式化
│   │   └── usb_task.rs         # UsbLink 传输驱动、环形行缓存与主机失联 10 秒看门狗
│   ├── ui/                     # 240x240 圆形人机交互界面
│   │   ├── mod.rs              # 颜色调色板、UiState 统一快照与 ViewMode 视图枚举
│   │   ├── island_view.rs      # Mode A：桌面灵动岛视图（动态胶囊、进度脉冲、系统仪表弧）
│   │   ├── radar_view.rs       # Mode B：2.4GHz 空间态势极坐标雷达视图（信道拥堵柱状图+扫描罗盘）
│   │   ├── clock_view.rs       # 待机机械表盘时钟视图（三角函数无死角刻度指针）
│   │   └── selftest_view.rs    # Stage 1 硬件自检仪表盘（实时传感器裸值可视化）
│   ├── radio/                  # 2.4GHz 无线射频感知
│   │   ├── mod.rs              # RadioReport 数据结构与无线通道定义
│   │   ├── ble_presence.rs     # BLE 5 目标设备追踪：EMA 信号滤波与 Near/Away 滞后门限
│   │   └── wifi_radar.rs       # Wi-Fi 2.4GHz 1~13 信道拥堵热度模型（20MHz 邻频重叠泄漏累加）
│   └── storage/                # Flash 存储与黑匣子
│       └── mod.rs              # 16MB Flash 分区读写：4MB 处持久化配置与 64B 环形无损黑匣子快照
└── tools/
    └── host_agent.py           # 电脑端全功能 Python 守护脚本（遥测流送、时钟对齐、离席自动锁屏）
```

---

## 3. 分模块架构设计文档索引

点击链接可深入查阅各子系统的架构原理与具体代码实现细节：

1. **[01. 硬件外设与板级支持包（BSP）架构](./01-hardware-and-bsp.md)**
   - 240x20 分带渲染引擎如何解决 112.5KB 全屏显存与 MCU 内存竞争；
   - QMI8658 12 字节连读突发性能优化；
   - PCF85063 硬件与软件时钟自适应回退机制；
   - 供电分压采样与 ETA6098 充电状态检测；
   - `GPIO24..GPIO30` 编译期强类型引脚所有权封印模式。
2. **[02. 物理姿态解算与手势识别引擎](./02-gestures-and-physics.md)**
   - 一阶低通滤波重力向量提取与零漂抑止；
   - 基于急动度（Jerk）的高精度防误触敲击检测状态机；
   - 左右倾斜滞后回差比较与死区锁定；
   - 屏幕朝下扣放（FaceDown）免打扰检测；
   - BOOT 按键防重入长按闭锁与双击消抖状态机。
3. **[03. USB 双向协议栈与电脑守护服务](./03-protocol-and-host.md)**
   - 行优先 ASCII 紧凑协议规范（`@ISLAND:`）；
   - 基于 `core::fmt::Display` 的零堆分配协议序列化；
   - 容错式键值对解析引擎；
   - 跨平台 Python 守护进程（VID:PID 自动协商、系统遥测、时区对齐与 OS 自动锁屏）。
4. **[04. 圆屏 UI 引擎与多视图渲染系统](./04-ui-and-graphics.md)**
   - 极客暗黑高对比度 RGB565 调色板；
   - 动态灵动岛胶囊状态流转（Thinking 脉冲、WaitConfirm 呼吸、Success/Error 告警）；
   - 极坐标态势雷达与信道频谱柱状图；
   - 经典机械表盘三角几何学运算；
   - 硬件自检仪表板。
5. **[05. 2.4GHz 空间射频态势感知与近场交互](./05-radio-sensing.md)**
   - 2.4GHz Wi-Fi 20MHz 邻频重叠泄漏累加模型；
   - BLE 5 目标设备追踪与 EMA 指数滑动平均滤波；
   - Near/Away 滞后迟滞比较与离席超时看门狗。
6. **[06. 异步并发模型与存储可靠性体系](./06-concurrency-and-storage.md)**
   - Embassy 5 大异步并发协程任务设计；
   - 单消费者 Actor 模式解决 MPMC 通道竞争问题；
   - 16MB Flash 分区布局与零大栈帧（Zero-Stack-Frame）硬件擦写；
   - 64 字节循环黑匣子持久化容灾机制；
   - `#![deny(warnings)]` 与全链路代码质量保证。
