# 05. BLE 在席、HID 与音乐联动

ESP32-C6 的 BLE 射频是设备自主运行的可选连接层。扫描、在席、GATT、HID 和音乐输入共享 BLE 资源，但不能阻塞宠物状态机或渲染任务。

---

## 1. 在席感知

### 1.1 目标绑定与 RSSI 滤波

`BlePresenceTracker` 使用 16 位地址折叠哈希、整数 EMA 和滞回策略；目标绑定命令通过
`RADIO_TARGET_SIGNAL` 进入扫描任务：

- `Near`：EMA RSSI >= -68 dBm；
- `Away`：EMA RSSI < -78 dBm；
- 中间 10 dB 保持原状态；
- 目标 15 秒未见时进入 `Away`；
- 未绑定目标时不产生虚假的在席事件。

在席结果只驱动设备本地行为：

- `Near`：恢复到配置的正常背光亮度；
- `Away`：将背光降到安静策略的亮度，宠物本地动画仍由 UI 状态决定；
- 在席结果只用于上述本地策略，不产生外部操作事件。

### 1.2 事件边沿

当前实现每秒发布一个完整 `RadioReport`，其中包含 `PresenceState`、RSSI、目标可见性和
固定容量的广播摘要；没有单独的 `PresenceTransition` 类型。

---

## 2. BLE GATT 外设

设备作为 BLE 外设提供 HID Service 和 MusicScene Service，手机或电脑仅在需要时连接。

当前版本没有配置、统计或黑匣子 GATT 特征；设备配置由启动时的 Flash 记录加载。

---

## 3. BLE HID

首版实现标准 HID over GATT Profile，报告范围限定为 Consumer Control：

- 播放/暂停；
- 上一首/下一首；
- 音量增加/减少；
- 静音；

实现约束：

- `HidAction` 经过固定容量队列发送，断开时丢弃过期瞬时事件；
- 只发送 Consumer Control 报告，不发送键盘文本或鼠标事件；
- 报告失败仅记录诊断结果；
- BLE HID 连接不是宠物运行的前置条件。

HID 与 MusicScene 共用同一个 GATT server；属性表预留两个连接，当前外设任务逐个处理接受的
连接。任一连接断开都必须让本地 `PetBehavior` 继续运行。

---

## 4. 音乐联动

### 4.1 MusicScene GATT 特征

播放端按需分析音乐后，向自有 128 位 UUID 的写入特征发送固定 10 字节帧：

```text
MusicScene {
    version: u8,           // 当前为 1
    sequence: u16,         // little-endian
    bpm: u16,              // 40..240；0 表示未知
    beat_phase: u8,        // 0..255
    energy: u8,            // 0..100
    mood: MusicMood,       // Calm/Happy/Focused/Excited/Sad/Unknown
    section: MusicSection, // Intro/Verse/Chorus/Bridge/Outro/Unknown
    flags: u8,
}
```

- `sequence` 防止乱序帧覆盖最新状态；
- 数据间隔超过 3 秒转为 `Inactive`；
- `MusicScene` 只影响动作强度、表情和短剧情，不覆盖 FaceDown、睡眠或用户手势；
- 原始音频、歌曲标题和外部账户信息不存入设备。

---

## 5. BLE 并发与功耗

- BLE HCI runner 独占控制器，扫描、GATT 和 HID 业务通过固定容量事件通道解耦；
- 在席扫描使用 250ms 间隔、50ms 窗口的低占空比会话；
- Away、FaceDown 和低电量时由本地策略降低背光或动画活动；
- BLE 连接异常不得让单核任务忙轮询，所有等待分支必须可取消并让度；
- MusicScene 写入失败只返回 ATT 错误，不重启设备或阻塞 UI。
