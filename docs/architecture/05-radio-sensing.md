# 05. BLE 5 近场感知与离席自动安全锁屏

近场感知子系统位于 [`src/radio/`](../../src/radio/)，专精于利用 ESP32-C6 内置的 **Bluetooth 5.0 (BLE)** 硬件射频能力，实现“用户佩戴智能手表/手机走近桌面时自动亮屏、离开工位时自动锁屏”的桌面安全防护闭环。

---

## 1. BLE 5 目标设备追踪与近场在席检测（`src/radio/ble_presence.rs`）

为了实现无感、精准的工位在席识别，[`BlePresenceTracker`](../../src/radio/ble_presence.rs) 实现了抗抖动高可靠的在席检测状态机。

### 1.1 16位紧凑 MAC 散列
为了在 `#![no_std]` 且固定长度的数据结构中以极小开销追踪设备，将 6 字节蓝牙物理 MAC 地址通过异或折叠散列为 16 位整型：
$$\text{Hash} = (\text{MAC}_0 \oplus \text{MAC}_2 \oplus \text{MAC}_4) \mid \Big((\text{MAC}_1 \oplus \text{MAC}_3 \oplus \text{MAC}_5) \ll 8\Big)$$

### 1.2 EMA 指数滑动平均滤波
无线信号在室内多径传播环境下瞬时抖动剧烈（可能在 1 秒内发生 $\pm 15\text{dB}$ 的剧烈浮动）。直接判定会导致离席检测频繁闪烁。

系统采用 **一阶指数滑动平均滤波器（EMA）**：
$$\overline{\text{RSSI}}_k = \alpha \cdot \text{RSSI}_k + (1 - \alpha) \cdot \overline{\text{RSSI}}_{k-1}$$
其中 $\alpha = 0.25$。该滤波参数赋予最新信号 $25\%$ 的响应权重，同时保留 $75\%$ 的历史基线，能够平滑过滤人走动引起的偶发信号下陷，同时在用户真正走近或离开时保持在 1.5 秒内敏捷跟进。

### 1.3 具有 10dB 回差的滞后比较器（Hysteresis Thresholds）
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

### 1.4 15 秒断联失联看门狗
若用户直接带上手机走出办公室门，广播信号骤停（不再产生新的广播包）。此时系统检查距上次捕获该目标信标的时间：
$$\Delta t = \text{now} - \text{last\_seen\_target}$$
绑定目标一旦在 15 秒内从未被观测到，状态机自动判定为 `AWAY`，避免把“目标消失”误判为在席。

### 1.5 广播采集与事件闭环
[`src/bin/main.rs`](../../src/bin/main.rs) 中的 `ble_runner_task` 驱动 Trouble Host 的 HCI 接收循环；`ble_scan_task` 负责启动并周期性续期 100ms 被动扫描会话。扫描处理程序将广播地址与 RSSI 投递到固定容量通道，由 `radio_presence_task` 消费并送入状态机处理。

`radio_presence_task` 在在席状态发生跳变时生成设备事件，经由异步通道交给 USB 发送任务；因此电脑端 `host_agent.py` 的离席锁屏路径与底层扫描数据完整闭环。未绑定目标时不会产生冗余在席事件。

为确保 RSSI 更新频率不受控制器去重策略影响，扫描会话关闭重复广播过滤；同一地址的新广播会持续刷新 15 秒看门狗。初始绑定目标尚未被观测到时保持 `AWAY`，避免把“尚未发现”误报为在席。

### 1.6 严格分层与实体所有权（Layering Integrity）
在模块分层设计中，信号信标结构体定义严格归属于 [`src/radio/mod.rs`](../../src/radio/mod.rs)，[`BlePresenceTracker`](../../src/radio/ble_presence.rs) 内部消费。UI 模块仅作为下游表现层只读展示雷达点阵，杜绝了底层射频感知反向依赖上层界面的分层倒置缺陷。
