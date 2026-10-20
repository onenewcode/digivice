# 03. USB 双向协议栈与电脑守护服务

通信子系统位于板端的 [`src/proto/`](file:///Volumes/HIKSEMI/code/island/src/proto/) 与电脑端的 [`tools/host_agent.py`](file:///Volumes/HIKSEMI/code/island/tools/host_agent.py)，负责在 ESP32-C6 与开发机之间建立全双工、零延迟、结构化的双向通信桥梁。

---

## 1. 信道复用与协议设计规约

### 1.1 一线双用与日志共存原理
微雪 ESP32-C6 开发板使用原生 **USB-Serial-JTAG (`GPIO12`/`GPIO13`)**，同时承担三项职责：
1. **电源供电**；
2. **`esp-println` 文本调试日志输出**；
3. **结构化机器协议双向通信**。

为了在无外部专用通信串口芯片的前提下，使开发者在串口终端中直观阅读调试日志，同时让守护进程准确捕获结构化机器帧，系统采用了 **行前缀 ASCII 协议（Line-Delimited Frame Protocol）**：
- **普通日志行**：凡不以 `@ISLAND:` 开头的文本行，电脑守护端一律作为调试日志原样输出，便于开发诊断。
- **协议数据行**：以 `@ISLAND:` 开头、以 `\n` 结尾的文本行，守护进程立即拦截并解析。

---

## 2. 帧格式与协议语法

### 2.1 下行协议帧（Host PC $\rightarrow$ ESP32-C6 Device）

1. **系统遥测帧（`SYS`，以 2Hz 频率下发）**
   ```text
   @ISLAND:SYS,cpu=42,ram=68,rx_kbps=1240,tx_kbps=320,temp=56\n
   ```
   - `cpu`：Host CPU 总体使用率（$0..100\%$）；
   - `ram`：Host 内存使用率（$0..100\%$）；
   - `rx_kbps` / `tx_kbps`：Host 实时下行 / 上行网络带宽（KB/s）；
   - `temp`：CPU 核心温度（°C）。

2. **灵动岛任务状态帧（`ACT`，事件驱动推送）**
   ```text
   @ISLAND:ACT,state=WAIT_CONFIRM,source=Cargo,title=Run cargo test?,progress=100\n
   ```
   - `state`：状态枚举（`IDLE`, `THINKING`, `WAIT_CONFIRM`, `SUCCESS`, `ERROR`）；
   - `source`：任务来源标识（如 `Cargo`, `Codex`, `GitHub`, `GitLab`）；
   - `title`：任务简要说明（支持包含逗号与空格）；
   - `progress`：进度百分比（$0..100$）。

3. **时间同步帧（`SYNC`，开机及每分钟定期对齐）**
   ```text
   @ISLAND:SYNC,epoch=1792488000,tz=480,media=Active\n
   ```
   - `epoch`：Host 当前 Unix 时间戳（秒）；
   - `tz`：Host 本地时区偏移（分钟，如东八区为 $+480$）；
   - `media`：当前播放媒体状态或标签。

4. **黑匣子导出指令（`DUMP_LOGS`）**
   ```text
   @ISLAND:DUMP_LOGS\n
   ```

---

### 2.2 上行协议帧（ESP32-C6 Device $\rightarrow$ Host PC）

当板端发生物理敲击、姿态翻转或存在感应变化时，立即向 Host 上报事件帧：

1. **双击审批事件（`DOUBLE_TAP`）**
   ```text
   @ISLAND:EVT,type=DOUBLE_TAP,mode=ISLAND,context=WAIT_CONFIRM\n
   ```
   用户在屏幕边缘双击，表示批准当前的 AI 指令执行或代码发布。
2. **单击消除事件（`SINGLE_TAP`）**
   ```text
   @ISLAND:EVT,type=SINGLE_TAP,mode=ISLAND,context=NOTIFY_CLEAR\n
   ```
   消除已完成的任务提醒。
3. **屏幕朝下扣放免打扰事件（`FACE_DOWN`）**
   ```text
   @ISLAND:EVT,type=FACE_DOWN,state=ENTER\n
   @ISLAND:EVT,type=FACE_DOWN,state=EXIT\n
   ```
4. **用户在席/离席感知事件（`PRESENCE`）**
   ```text
   @ISLAND:EVT,type=PRESENCE,state=AWAY,rssi=-82\n
   @ISLAND:EVT,type=PRESENCE,state=NEAR,rssi=-54\n
   ```
5. **设备健康心跳帧（`HEARTBEAT`，每 5 秒发送一次）**
   ```text
   @ISLAND:EVT,type=HEARTBEAT,v_rail=4185,uptime=3600\n
   ```
   上报当前真实的供电轨电压（mV）与固件连续运行秒数。

---

## 3. 固件端零分配序列化架构（`src/proto/`）

传统嵌入式开发中，开发者常通过逐段 `push_str` 拼接字符串或手工实现整型转字符，代码冗长且极易出现索引越界。

在 [`src/proto/mod.rs`](file:///Volumes/HIKSEMI/code/island/src/proto/mod.rs) 中，我们利用 Rust 标准的 `Display` 特征与 `heapless::String` 提供的 `core::fmt::Write` 实现进行重构：

```rust
impl core::fmt::Display for DeviceEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{PROTO_PREFIX}")?;
        match self {
            DeviceEvent::DoubleTap { context } => {
                writeln!(f, "EVT,type=DOUBLE_TAP,mode=ISLAND,context={context}")
            }
            DeviceEvent::SingleTap { context } => {
                writeln!(f, "EVT,type=SINGLE_TAP,mode=ISLAND,context={context}")
            }
            DeviceEvent::FaceDown { entering } => {
                let state = if *entering { "ENTER" } else { "EXIT" };
                writeln!(f, "EVT,type=FACE_DOWN,state={state}")
            }
            DeviceEvent::Presence { is_near, rssi } => {
                let state = if *is_near { "NEAR" } else { "AWAY" };
                writeln!(f, "EVT,type=PRESENCE,state={state},rssi={rssi}")
            }
            DeviceEvent::Heartbeat { v_rail, uptime_sec } => {
                writeln!(f, "EVT,type=HEARTBEAT,v_rail={v_rail},uptime={uptime_sec}")
            }
            DeviceEvent::DumpLogsRequest => {
                writeln!(f, "LOGS_DUMP_START")
            }
        }
    }
}
```

在发送端 [`src/proto/usb_task.rs`](file:///Volumes/HIKSEMI/code/island/src/proto/usb_task.rs) 中，事件发送直接精炼为 5 行安全代码：
```rust
pub async fn send_event(&mut self, evt: &DeviceEvent) {
    let mut msg = String::<96>::new();
    if write!(msg, "{evt}").is_ok() {
        self.send_str(&msg).await;
    }
}
```
- **纯 `#![no_std]` 零堆分配**：完全在栈上 96 字节静态缓冲区内完成编码；
- **传输原子性**：单条完整事件帧一次性写入 USB TX FIFO，防止多任务交错打印引发帧断裂。

---

## 4. 电脑端全功能守护服务（`tools/host_agent.py`）

电脑端 Python 守护进程充当终端的后端调度引擎：

```mermaid
flowchart TD
    subgraph Host["Host Agent (Python)"]
        Scanner["自动发现 USB 设备 (VID: 0x303A, PID: 0x1001)"]
        Telem["psutil 采集 (CPU, RAM, Net RX/TX, Temp)"]
        TimeSync["时钟同步器 (Unix Epoch + 本地时区)"]
        Dispatcher["事件分发器 (handle_device_event)"]
        OS_Lock["操作系统锁屏执行器 (macOS / Linux / Windows)"]
    end

    subgraph Hardware["ESP32-C6 Device"]
        USB_Serial["USB-Serial-JTAG 物理串口"]
    end

    Scanner -->|建立串口连接 115200| USB_Serial
    Telem -->|2Hz @ISLAND:SYS| USB_Serial
    TimeSync -->|开机/@60s @ISLAND:SYNC| USB_Serial
    USB_Serial -->|@ISLAND:EVT,type=PRESENCE,state=AWAY| Dispatcher
    Dispatcher -->|触发锁屏| OS_Lock
```

### 4.1 跨平台原生锁屏机制
当用户离开工位触发 `state=AWAY` 时，守护服务根据运行操作系统调用底层接口立即锁屏：
- **macOS**：执行 `pmset displaysleepnow`，瞬间关闭屏幕进入睡眠锁定；
- **Linux**：调用 systemd 接口 `loginctl lock-session`；
- **Windows**：调用系统动态链接库 `ctypes.windll.user32.LockWorkStation()`。

### 4.2 CLI 快速调试与状态注入
守护脚本支持单次快速注入指令，极大加速了开发者调试：
```bash
# 模拟编译等待批准通知
python3 tools/host_agent.py --notify "Approve git push to prod?" --source "GitLab" --state "WAIT_CONFIRM"

# 模拟编译成功
python3 tools/host_agent.py --notify "All tests passed" --source "Cargo" --state "SUCCESS"

# 模拟构建报错
python3 tools/host_agent.py --notify "Build failed: syntax error" --source "Rustc" --state "ERROR"
```
