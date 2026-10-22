# 05. BLE 在席、配置、HID 与音乐联动

ESP32-C6 的 BLE 射频从“通知电脑锁屏”的从属链路，升级为设备自主运行的可选连接层。扫描、在席、GATT、HID 和音乐输入必须共享 BLE 资源，但不能阻塞宠物状态机或渲染任务。

---

## 1. 在席感知

### 1.1 目标绑定与 RSSI 滤波

现有 `BlePresenceTracker` 的 16 位地址折叠哈希、EMA 和滞回策略继续保留，目标绑定通过 GATT 完成：

- `Near`：EMA RSSI >= -68 dBm；
- `Away`：EMA RSSI < -78 dBm；
- 中间 10 dB 保持原状态；
- 目标 15 秒未见时进入 `Away`；
- 未绑定目标时不产生虚假的在席事件。

在席结果只驱动设备本地行为：

- `Near`：恢复亮度、欢迎动作、允许音乐/HID 增强；
- `Away`：降低背光、进入安静策略、停止非必要动画；
- 不再发送“锁屏电脑”事件，也不依赖任何宿主程序。

### 1.2 事件边沿

`PresenceState` 只在边沿变化时发布 `PresenceTransition`：

```text
PresenceTransition::Arrived
PresenceTransition::Departed
PresenceTransition::Unknown
```

`Unknown` 用于扫描暂时不可用或目标配置被清除，不能直接等同 `Away`。

---

## 2. BLE GATT 配置服务

设备作为 BLE 外设提供标准 GATT 服务，手机或电脑仅在需要时连接。

### 2.1 配置内容

- 宠物偏好：亮度、动画速度、互动灵敏度；
- FocusSession：专注、短休息、长休息时长；
- HID：启用状态、手势到动作映射、配对清除；
- MusicScene：启用状态、动作风格、超时策略；
- 在席：目标绑定、门限和离席时长；
- 只读信息：固件版本、电池、电源、统计和黑匣子摘要。

### 2.2 一致性与安全

- GATT 写入先验证范围和版本，再发送到 `STORAGE_COMMAND_CHANNEL`；
- `storage_task` 用 Sector A/B 双缓冲提交，成功后才更新 RAM 配置；
- 连接中断不会回滚已经确认的配置；
- 写入错误只返回错误状态，不重启、不阻塞 UI；
- 统计和黑匣子按需读取，不持续广播敏感数据。

---

## 3. BLE HID

首版实现标准 HID over GATT Profile，报告范围限定为 Consumer Control：

- 播放/暂停；
- 上一首/下一首；
- 音量增加/减少；
- 静音；
- 专注开始/暂停/完成映射到可配置的媒体动作。

实现约束：

- `HidConnectionState` 使用 `Unpaired/Paired/Connected` 强类型状态；
- `HidAction` 经过固定容量队列发送，断开时丢弃过期瞬时事件；
- 不发送键盘文本、不移动鼠标、不执行系统锁屏；
- 报告失败仅记录诊断结果；
- BLE HID 连接不是宠物运行的前置条件。

BLE HID 与音乐 GATT 可以同时存在，但任何一个连接断开都必须让本地 `PetBehavior` 继续运行。

---

## 4. 音乐联动

### 4.1 BLE MIDI

设备可接收 BLE MIDI Clock、Start/Stop/Continue 和 Note On/Off。MIDI 事件只提供拍速和拍点，不承载音频。

系统级音乐（例如流媒体播放器）未必会输出 MIDI Clock；需要手机/电脑上的临时应用、插件或音乐软件把音频转换为 MIDI/特征。该工具可以按需启动，不能成为设备运行依赖。

### 4.2 MusicScene GATT 特征

对于不支持 BLE MIDI 的音乐分析器，提供写入特征：

```text
MusicScene {
    bpm: u16,              // 40..240；0 表示未知
    beat_phase: u8,        // 0..255
    energy: u8,            // 0..100
    mood: MusicMood,       // Calm/Happy/Focused/Excited/Sad/Unknown
    section: MusicSection, // Intro/Verse/Chorus/Bridge/Outro/Unknown
    sequence: u16,
}
```

- `sequence` 防止乱序帧覆盖最新状态；
- 数据间隔超过 3 秒转为 `Inactive`；
- `MusicScene` 只影响动作强度、表情和短剧情，不覆盖 FaceDown、睡眠或用户手势；
- 原始音频、歌曲标题和外部账户信息不存入设备。

---

## 5. BLE 并发与功耗

- BLE HCI runner 独占控制器，扫描、GATT、HID 和 MIDI 业务通过固定容量事件通道解耦；
- 在席扫描采用低占空比策略，HID/GATT 连接建立后暂停不必要的广播扫描；
- Away、FaceDown 和低电量时降低扫描、广告和动画频率；
- BLE 连接异常不得让单核任务忙轮询，所有等待分支必须可取消并让度；
- BLE 配对和配置变化通过黑匣子记录原因，便于离线排查。
