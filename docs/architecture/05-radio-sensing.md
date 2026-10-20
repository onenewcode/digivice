# 05. 2.4GHz 空间射频态势感知与近场交互

无线射频态势感知子系统位于 [`src/radio/`](file:///Volumes/HIKSEMI/code/island/src/radio/)，利用 ESP32-C6 内置的 **Wi-Fi 6 (802.11ax) + Bluetooth 5.0 (BLE)** 硬件射频控制器与共存引擎（Coexistence），将桌面周边的电磁环境量化并转化为动态态势指示。

---

## 1. 2.4GHz 频谱物理特性与信道重叠泄漏模型

在 2.4GHz ISM 频段中，标准 802.11b/g/n/ax 信号采用 **20MHz 频宽**。然而，2.4GHz 标准定义的信道中心频率间隔仅有 **5MHz**（信道 1 为 2412MHz，信道 2 为 2417MHz……信道 13 为 2472MHz）。

这意味着一个在信道 6 发射的 20MHz Wi-Fi 信号，其实际频谱能量不仅覆盖信道 6，还会严重外溢泄漏至信道 4、5 与信道 7、8！

```text
信道中心:   Ch1   Ch2   Ch3   Ch4   Ch5   Ch6   Ch7   Ch8   Ch9  Ch10  Ch11  Ch12  Ch13
频宽范围: └─────── 20MHz ───────┘     └─────── 20MHz ───────┘
                                   ▲
                             邻频频谱重叠泄漏
```

### 1.1 邻频泄漏累加算法（`src/radio/wifi_radar.rs`）
为了真实反映电磁空间中的实际信道干扰程度，[`WifiRadarAnalyzer`](file:///Volumes/HIKSEMI/code/island/src/radio/wifi_radar.rs) 建立了物理级频宽外溢累加模型：

1. **RSSI 贡献分值计算**：
   将扫描到的每个 AP 信号强度（$-100\text{dBm} \sim -30\text{dBm}$）线性映射为基础贡献权重：
   $$\text{BaseScore} = \max(\text{RSSI}, -100) + 100 \quad (0 \sim 70)$$
2. **频谱泄漏加权注入**：
   对于处于信道 $C$（索引 $i = C-1$）的 AP：
   - **中心信道 $i$**：接收全额权重 $100\%$：
     $$\text{Score}[i] += \text{BaseScore}$$
   - **相邻 $\pm 1$ 信道（$i \pm 1$）**：接收 $70\%$ 的主瓣外溢能量：
     $$\text{Score}[i \pm 1] += \frac{\text{BaseScore} \times 7}{10}$$
   - **次相邻 $\pm 2$ 信道（$i \pm 2$）**：接收 $30\%$ 的旁瓣外溢能量：
     $$\text{Score}[i \pm 2] += \frac{\text{BaseScore} \times 3}{10}$$
3. **归一化百分比输出**：
   扫描结束后，统计 13 个信道中的最高得分 $\text{MaxScore}$，将所有信道得分线性映射为 $0 \sim 100\%$ 的标准拥堵度：
   $$\text{Congestion}[i] = \min\left(100, \frac{\text{Score}[i] \times 100}{\text{MaxScore}}\right)$$

该模型真实重现了周边路由器在相邻信道引发的频谱互斥现象，为雷达柱状图提供科学的电磁态势依据。

---

## 2. BLE 5 目标设备追踪与近场在席检测（`src/radio/ble_presence.rs`）

为了实现“用户佩戴智能手表/手机走近桌面时自动亮屏、离开工位时自动锁屏”的智能桌面交互，[`BlePresenceTracker`](file:///Volumes/HIKSEMI/code/island/src/radio/ble_presence.rs) 实现了抗抖动高可靠的在席检测状态机。

### 2.1 16位紧凑 MAC 散列
为了在 `#![no_std]` 且固定长度的雷达光斑表中以极小开销追踪设备，将 6 字节蓝牙物理 MAC 地址通过异或折叠散列为 16 位整型：
$$\text{Hash} = (\text{MAC}_0 \oplus \text{MAC}_2 \oplus \text{MAC}_4) \mid \Big((\text{MAC}_1 \oplus \text{MAC}_3 \oplus \text{MAC}_5) \ll 8\Big)$$

### 2.2 EMA 指数滑动平均滤波
无线信号在室内多径传播环境下瞬时抖动剧烈（可能在 1 秒内发生 $\pm 15\text{dB}$ 的剧烈浮动）。直接判定会导致离席检测频繁闪烁。

系统采用 **一阶指数滑动平均滤波器（EMA）**：
$$\overline{\text{RSSI}}_k = \alpha \cdot \text{RSSI}_k + (1 - \alpha) \cdot \overline{\text{RSSI}}_{k-1}$$
其中 $\alpha = 0.25$。该滤波参数赋予最新信号 $25\%$ 的响应权重，同时保留 $75\%$ 的历史基线，能够平滑过滤人走动引起的偶发信号下陷，同时在用户真正走近或离开时保持在 1.5 秒内敏捷跟进。

### 2.3 具有 10dB 回差的滞后比较器（Hysteresis Thresholds）
为了杜绝边界信号下的反复“在席/离席”震荡，状态机设置了两个非对称阈值门限：
- **靠近判准门限（Near Threshold）**：$\overline{\text{RSSI}} \ge -68\text{dBm}$ 时，判定进入 `NEAR` 在席状态；
- **离开判准门限（Away Threshold）**：$\overline{\text{RSSI}} < -78\text{dBm}$ 时，才判定进入 `AWAY` 离席状态；
- **$10\text{dB}$ 保护死区（Deadband）**：当信号在 $-68\text{dBm} \sim -78\text{dBm}$ 之间徘徊时，严格保持原有状态不变。

```text
 信号强度 (dBm)
    ▲
    │   -50 dBm ─── [在席 NEAR]
    │   -68 dBm ─── 靠近门限 (必须高于此线才触发 NEAR)
    │           ░░░ 10dB 迟滞缓冲区 (保持原状态不翻转) ░░░
    │   -78 dBm ─── 离开门限 (必须低于此线才触发 AWAY)
    │   -90 dBm ─── [离席 AWAY -> 触发电脑锁屏]
    ▼
```

### 2.4 15 秒断联失联看门狗
若用户直接带上手机走出办公室门，广播信号骤停（不再产生新的广播包）。此时 `update_watchdog` 检查距上次捕获该目标信标的时间：
$$\Delta t = \text{now} - \text{last\_seen\_target}$$
若 $\Delta t > 15\text{s}$，强制判定为 `is_near = false`，立即向 USB 串口上报 `@ISLAND:EVT,type=PRESENCE,state=AWAY`，驱动电脑守护脚本自动执行屏幕休眠锁定。
