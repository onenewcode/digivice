# 05. BLE 在席、HID 与音乐联动

ESP32-C6 的 BLE 射频是设备自主运行的可选连接层。扫描、在席、GATT、HID 和音乐输入共享 BLE 资源，但不能阻塞宠物状态机或渲染任务。

---

## 1. 在席感知

### 1.1 目标绑定与 RSSI 滤波

`BlePresenceTracker` 使用 16 位地址折叠哈希、整数 EMA 和滞回策略；目标绑定命令通过
`RADIO_TARGET_SIGNAL` 进入扫描任务：

- `Away`：使用持久配置的 `rssi_away_threshold`（合法范围 -120..=0）；Near 阈值为 Away + 10dB，
  保持滞回；
- 中间 10 dB 保持原状态；
- 目标 15 秒未见时进入 `Away`；
- 同一 15 秒 watchdog 同时将 `TargetVisibility` 置为 `Hidden`、RSSI EMA 重置为 -85，并清空
  当前 blip 表，防止诊断或 `bind_strongest` 复用过期目标观测；这是一代观测整体回收，
  不是每个 blip 的独立时间戳 TTL。
- 未绑定目标时不产生虚假的在席事件。
- **16 位地址折叠碰撞与手机私有地址边界**：
  - 16 位哈希算法（`TargetBinding::hash_addr`）将 48 位（6 字节）MAC 地址经两两 XOR 折叠压缩至 16 位：
    `((a0 ^ a2 ^ a4) as u16) | (((a1 ^ a3 ^ a5) as u16) << 8)`；
  - **碰撞边界**：由生日悖论，在同时存在约 300 台广播设备的密集射频环境下，存在约 50% 的 16 位哈希假阳性碰撞概率；不同 MAC（如 `[1,2,3,4,5,6]` 与 `[7,0,0,0,0,0]`）均可得到相同 hash=7。因此该哈希**仅作为本地机会主义邻近唤醒启发式因子，严禁用作设备身份凭证或安全认证**；
  - **手机私有地址轮换 (RPA)**：主流智能手机（iOS / Android）在后台锁屏广播时使用 BLE 隐私 1.2 可解析私有地址（Resolvable Private Address, RPA），MAC 地址每 15 分钟强制轮换一次。在此机制下，未配对的手机在地址轮换后哈希必然改变，固件将在 15 秒后安全收敛至 `Away`。对于长期在席感知，推荐配合固定静态地址的桌面 Beacon、外设 Tag 或受控应用伴侣服务使用；不支持将轮换 RPA 手机假定为固定静态身份。

在席结果只驱动设备本地行为：

- `Near`：恢复到配置的正常背光亮度；
- `Away`：将背光降到安静策略的亮度，宠物本地动画仍由 UI 状态决定；
- 在席结果只用于上述本地策略，不产生外部操作事件。

### 1.2 事件边沿

当前实现每秒发布一个完整 `RadioReport`，其中包含 `PresenceState`、RSSI、目标可见性和
固定容量的广播摘要；没有单独的 `PresenceTransition` 类型。

### 1.3 无线运行期韧性与局部降级

- **可选外设启动解耦**：BLE 控制器 (`BleConnector::new`) 初始化失败不再触发 `expect` 致命宕机。若无线硬件控制器不可用，固件优雅进入本地离线模式，跳过 runner/scan/peripheral 任务创建，不阻塞本地 RTC、按键、IMU 姿态、存储与 UI 渲染生命周期；
- **类型化健康状态与信号**：`RadioStatus` 明确区分 `Active`（正常运行）、`Disabled`（本地离线）、`ControllerUnavailable`（启动控制器探测失败）、`GattFailed`（GATT 服务注册异常）、`RunnerExited`（HCI runner 异常退出）。Runner 与 GATT 异常通过 `RADIO_STATUS_SIGNAL` 实时通知 `radio_presence_task`；
- **状态同步与背光保护**：非 `Active` 状态下在席自动置为 `Away` 且目标置为 `Hidden`，并立即清空过期观测；`AppStateMachine` 仅在 `radio_status.is_operational()` 为真时才将 `Away` 作为背光调暗依据，彻底杜绝了因无线故障导致整机屏幕永久变暗的问题；
- **状态卡可见性**：Status 视图状态行根据 `RadioStatus` 分别呈现 `BLE:NEAR`、`BLE:AWAY`、`BLE:OFFLINE`、`BLE:NO_CTRL`、`BLE:GATT_ERR` 或 `BLE:RUN_ERR`，提供透明的端侧健康诊断。

### 1.4 无线热路径日志限频与广播会话生命周期保护

在高密度 BLE 广播与频繁离线交互环境下，射频子系统实施严格的命令限频与拥塞保护：

- **HCI 事件热路径零阻塞**：`AdvertisementHandler::on_adv_reports` 与 `on_ext_adv_reports` 彻底移除逐包 `println!` 调用；队列满时改用原子计数器（`ADV_DROPPED_PACKETS`），由 `radio_presence_task` 在 1 秒 Ticker 周期统一聚合输出丢包摘要，杜绝 USB UART 阻塞导致 HCI 数据接收丢失；
- **广播会话免受离线交互干扰**：外设任务等待连接时，直接以 `advertiser.accept().await` 持续驻留，不再将可丢弃的离线 `HID_ACTION_CHANNEL` 混入 `select`。离线状态下的多媒体按键或手势不再导致广播句柄反复销毁重建，彻底消除了 HCI 广告重启风暴；在建立连接时统一调用 `discard_pending_hid_actions` 清除陈旧离线操作；
- **扫描会话生命周期延长 (30x HCI 命令削减)**：扫描会话持续时间由以往激进的 1 秒延长至 30 秒（`BLE_SCAN_SESSION_DURATION`），避免每秒触发 HCI Start/Stop Scan，将控制器指令开销削减 96.7%，同时保持 50ms 窗口 / 250ms 间隔的 20% 占空比低功耗硬件扫描；
- **IPC 报告通道指数退避**：`RADIO_REPORT_CHANNEL` 满时仅在丢包数为 2 的幂次方时输出告警，防止队列背压时串口持续刷屏。

---

## 2. BLE GATT 外设

设备作为 BLE 外设提供 HID Service 和 MusicScene Service，手机或电脑仅在需要时连接。

当前版本没有配置、统计或黑匣子 GATT 特征；设备配置由启动时的 Flash 记录加载。

---

## 3. BLE HID

当前实现 HID Service 的 Consumer Control 报告路径，尚未完成整个 HOGP 合规及主机兼容性验收。
报告范围限定为：

- 播放/暂停；
- 上一首/下一首；
- 音量增加/减少；
- 静音；

实现约束：

- `HidAction` 经过固定容量队列发送，断开时丢弃过期瞬时事件；
- 只发送 Consumer Control 报告，不发送键盘文本或鼠标事件；
- Report 特征是两字节小端 usage，空闲/释放为 `[0, 0]`；ID=1 由 Report Reference 标识，不重复塞进通知，规范来源及回归见 03 章；
- 每次通知最多等待 250ms；释放优先，失败间隔 50ms、最多 3 次后请求断开，不在未释放时发送新按下；
- 订阅/暂停策略为每连接独立状态；Suspend/无订阅不发新报告，恢复后先中立、不重放旧动作；只支持 Report Protocol，不提供 Boot 切换或 RemoteWake；
- 日志只确认本地排队而非主机确认，异常路径实施释放重试与超时保护；
- BLE HID 连接不是宠物运行的前置条件。

HID 与 MusicScene 共用同一个 GATT server。根据架构决策与资源预算收敛，固件严格采用**单一活动主控互斥模型**（`connections_max = 1`）：在任意时刻，仅允许一台上位机（电脑桌面伴侣或手机）持有有效 GATT 连接；连接期间广播暂停，杜绝了多主控媒体事件路由歧义与多路 CCCD 内存膨胀。当前连接断开后，外设自动恢复广播并等待新连接接入。任一连接建立或断开都绝不影响本地 `PetBehavior` 的自主生命周期。

### 3.1 配对、加密与会话安全生命周期

- **安全等级**：设备配置 BLE 5.0 Security Mode 1 Level 2（无认证链路加密，即 Just Works，IO 能力为 `NoInputNoOutput`），提供空口 AES-128 CCM 传输防监听保护；
- **会话级临时密钥生命周期**：固件采用会话级临时安全协商（Ephemeral Session Security），不跨重启向片上 Flash 写入 LTK/IRK，避免非易失存储磨损与配对脏数据积累；断电重启后上位机重连自动触发透明 Just Works 重新协商；
- **会话退出与清理**：连接断开时，系统立即调用 `discard_pending_hid_actions` 并将 `HidSession` 彻底归零，确保不向下一个接入主机泄漏或重放陈旧动作；
- **信任边界声明**：由于设备不具备屏幕输码或矩阵按键确认能力，Just Works 机制无法防范主动中间人（MITM）攻击。因此，BLE HID 与在席感知仅作为本地便捷伴侣扩展，严禁假定为高安全等级的身份凭证或物理资产授权钥匙。

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
- 活动会话内只按 `sequence` 的 u16 回绕比较接受新帧；`Unknown` 情绪是合法字段，不能用它
  判断“尚无有效音乐”。超过 3 秒超时后，下一帧显式开启新会话并重新建立序列基准；旧会话
  的帧不能在活动窗口内覆盖当前状态。
- 数据间隔超过 3 秒转为 `Inactive`；
- 单调时间节拍推进：`MusicScene::current_beat_phase` 依据 BPM 与接收时刻的单调时间戳插值推进（$phase\_advance = \frac{elapsed\_ms \times bpm \times 256}{60000}$），40Hz 本地维护节拍与外圈霓虹视觉环即使在低频/抖动遥测下也能保持平滑连续推进；3 秒无新数据时节拍脉冲归零、自动退出跳舞动画并回到待机呼吸；睡眠与用户动作严格优先于音乐动画。
- `MusicScene` 只影响动作强度、表情和短剧情，不覆盖 FaceDown、睡眠或用户手势；
- 原始音频、歌曲标题和外部账户信息不存入设备。

---

## 5. BLE 并发与功耗

- BLE HCI runner 独占控制器，扫描、GATT 和 HID 业务通过固定容量事件通道解耦；
- 在席扫描使用 250ms 间隔、50ms 窗口的低占空比会话；
- Away、FaceDown 和低电量时由本地策略降低背光或动画活动；
- 只有 `VoltageValidity::Valid` 且滤波电压不高于 3500mV 才进入低电量背光档（正常亮度上限
  30%）；转换失败/越界读数不会触发或解除低电量策略，避免 ADC 故障伪造功耗状态。
- BLE 连接异常不得让单核任务忙轮询，所有等待分支必须可取消并让度；
- MusicScene 写入失败只返回 ATT 错误，不重启设备或阻塞 UI。
