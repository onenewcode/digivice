# 02. 物理姿态解算与手势识别引擎

手势识别与物理姿态处理模块位于 [`src/gesture/`](../../crates/digivice-core/src/gesture/)，包含姿态解算滤波器 [`filter.rs`](../../crates/digivice-core/src/gesture/filter.rs) 与事件检测状态机 [`detector.rs`](../../crates/digivice-core/src/gesture/detector.rs)。

惯性样本值类型 `ImuSample` 位于 `domain/imu.rs`，BSP 仅为兼容现有消费者重新导出；手势和滤波器不再依赖 BSP 或全局 IPC。`GestureDetector::new_at` 接收可注入的单调起点，运行期 `new` 仍使用 `Instant::now()`。`sh scripts/test-host.sh --locked` 在独立 host 测试包中直接编译这些固件源码，回放明确时间戳，不复制业务模型，也不向固件引入 `std`。脚本从临时工作目录调用 Cargo，避免继承固件的 RISC-V runner、`build-std` 和编译 flags；host 锁文件独立跟踪。

惯性样本仅包含加速度/角速度六轴；温度独立读取且不进入手势成功条件。生产高频路径不做
温度事务，自检约 1 秒读取一次用于诊断，故温度故障不会将合法六轴数据转为失败或零样本。
寄存器换算与异常重探测边界详见 01 章。

---

## 1. 姿态解算与滤波管线（`ImuFilter`）

`sensor_task` 轮询周期为 5ms（200Hz），调用 `ImuFilter` 处理成功六轴读取。
传感器在 6DOF 模式下 ODR 标称输出为 224.2Hz；连续流 data-ready 标志过滤重复读取，
滤波器使用相邻有效样本的实际 `Instant` 间隔；首样本、倒退时间或超过 100ms 的停顿以 `dt=0`
只更新重力 LPF、不跨停顿积分航向。
滤波器负责将 6 轴数据转换为重力向量、姿态角与航向角，不应把轮询成功视为新数据证明。

```mermaid
flowchart LR
    Sample["原始采样 (ax, ay, az, gx, gy, gz)"] --> Calib["减去零位漂移 (gx_bias, gy_bias, gz_bias)"]
    Calib --> LPF["一阶低通滤波 (alpha = 0.28)"]
    LPF --> Angles["快速三角姿态解算 (fast_atan2_deg)"]
    Angles --> Heading["平放死区航向积分 (|Gz|>750mg & |Gz_dps|>2.0)"]
    Heading --> Output["聚合姿态快照 (Attitude Snapshot)"]
```

### 1.1 开机静止校准

开机窗口最多观察 50 次成功六轴样本，使用有界 `StationaryCalibrator` 在线累计，不把样本
数组放到任务栈。至少 32 次有效样本才允许接受零偏；每个样本的重力模长必须在
850..=1150mg，任一轴角速度绝对值不得超过 5dps，且窗口内每轴零偏跨度不得超过 2000mdps。
重力异常、运动或样本不足分别返回结构化拒绝原因，旧 bias 不会被部分窗口覆盖；校准失败
只记录警告，传感器、按键、电源和 UI 继续运行。

这只是启动时的静止性门槛，不是在线温漂跟踪或完整六面校准；实际静止姿态、温度漂移和
用户运动期间的误报率仍需实机测量。若配置包含非零持久化 gyro bias，则优先使用该 bias
并跳过启动平均；`accel_bias_mg` 始终在重力滤波前减除。配置值来自已验证的 Flash 记录，
但仍不等于重新做六面校准。

### 1.1 一阶低通重力分离
当设备处于运动或桌面敲击状态时，原始加速度包含高频动态惯性力。为了准确得到设备相对于重力场的倾角，采用一阶数字低通滤波器（LPF）：
$$G_{k} = G_{k-1} + \alpha \cdot (A_k - G_{k-1})$$
其中滤波系数 $\alpha = 0.28$。在 200Hz 采样率下，该参数既能平滑滤除敲击产生的微高频杂波，又赋予了姿态解算灵敏敏捷的动态跟手性。

### 1.2 俯仰角与横滚角解算（Zero-FPU 快速有理逼近）
ESP32-C6 (RV32IMAC) 无硬件浮点单元。为避免在高频采样循环中调用高开销的 `libm::atan2f` 软件仿真库，姿态解算器采用了 Hastings 极值有理逼近算法（`fast_atan2_deg`）：
$$\theta(r) \approx \frac{57.29578 \cdot r}{1.0 + 0.28086 \cdot r^2} \quad (r \in [0, 1])$$
通过八分圆折叠映射，在全域 $360^\circ$ 范围内典型角度逼近误差低于 $0.3^\circ$，单次姿态角求解仅需 1 次浮点乘法与 1 次除法，显著降低单核 CPU 中断周期占用：
$$\text{Roll} = \text{fast\_atan2\_deg}(G_x, G_z)$$
$$\text{Pitch} = \text{fast\_atan2\_deg}(G_y, G_z)$$

### 1.3 消除漂移的死区航向梯形积分
传统仅依赖陀螺仪的航向角积分在静止时会因微小零漂不断自发旋转。`ImuFilter` 引入了**双重死区门限约束**：
1. **平放约束**：仅当屏幕正面向上且接近水平（$G_z > 750\text{mg}$）时才允许航向积分，翻转或悬空时不累加错误航向。
2. **旋转速度死区**：仅当角速度绝对值 $|\omega_z| > 2.0\text{ dps}$ 时才进行积分：
   $$\text{Heading}_{k} = \text{Heading}_{k-1} - \omega_z \cdot dt$$
   当旋转停止（$|\omega_z| \le 2.0\text{ dps}$）时完全冻结积分，彻底消除静止漂移。

### 1.4 航向事件的圆周差值

`GestureDetector` 发布 `HeadingUpdate` 前使用 `[0, 360)` 的最短圆弧距离，距离 1..=180°
才通过 50ms 节流。359→0、0→1 与 1→0→359 不会因普通绝对差值被卡住；这只修正事件
节流/去重，不改变滤波器的航向积分或 UI 快照频率。

### 1.5 宠物侧倾与重力滤波复用

`sensor_task` 经 `ImuAttitudeSnapshot` 将一阶低通滤波重力向量（$G_x, G_y, G_z$）送至应用层，`AppStateMachine` 提取该滤波重力驱动宠物的 `on_imu_attitude`，而不是直接传递原始未滤波加速度 `sample`。
宠物内部根据 Y-Z 平面下向重力分量 $G_{down} = \sqrt{G_y^2 + G_z^2}$ 计算侧倾角 $\text{fast\_atan2\_deg}(G_x, G_{down})$：
1. **俯仰无关性 (Pitch-Invariant)**：垂直拾起或调整屏幕倾角面对用户时（$G_y, G_z$ 变化但 $G_x \approx 0$），侧倾角严格保持为 $0^\circ$，不发生旋转；
2. **高频冲击隔离**：桌面平移与按键震动被 LPF 滤除，避免高频线性加速度尖峰直接注入宠物视线和侧倾角造成画面颤抖；
3. **死区抑制**：$|G_x| \le 60\text{mg}$ 保持中立，超过死区平滑跟踪。

---

## 2. 物理敲击（Tap）与急动度检测（Jerk Engine）

传统的敲击检测通常设置一个简单的加速度峰值阈值，但在日常使用中，用户拿起设备、移动设备或桌面轻微震动都会导致峰值误判。

### 2.1 急动度（Jerk）微分指标
为了精确识别“桌面对设备的硬质冲击波”，检测器计算**加速度变化率（急动度 Jerk）**：
$$\text{Jerk} = |A_{z, k} - A_{z, k-1}| + \frac{|A_{x, k} - A_{x, k-1}| + |A_{y, k} - A_{y, k-1}|}{2}$$
- 平滑拿起设备时：加速度绝对值很大，但 $\Delta A$ 很小，Jerk 极低。
- 手指正常敲击设备边缘或桌面时：产生陡峭的高频冲击应力波，$\text{Jerk} > 320$ 灵敏捕捉。

### 2.2 单击 / 双击识别状态机
```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> FirstTapDetected: Jerk > 320 (第1次脉冲)
    FirstTapDetected --> DoubleTap: 60ms <= dt <= 420ms 再次触发 Jerk > 320
    FirstTapDetected --> SingleTap: dt > 420ms 无二次敲击
    DoubleTap --> Cooldown: 输出双击事件 (DoubleTap)
    SingleTap --> Cooldown: 输出单击事件 (SingleTap)
    Cooldown --> Idle: 保持 180ms 忽略机械回弹余震
```

- **消抖死区（$< 60\text{ms}$）**：首次敲击引发的壳体机械振铃谐振通常在此时间窗内衰减，在此时间窗内忽略，防止单次敲击被误判为双击。
- **自然双击窗（$60\text{ms} \le \Delta t \le 420\text{ms}$）**：大幅放宽符合人类手指自然敲击节奏的时间窗口。
- **超时触发单击（$> 420\text{ms}$）**：窗口结束后确认为单次轻敲。
- **冷却窗口（$180\text{ms}$）**：输出事件后进入极短冷却期，兼顾高频连击响应与防余震抖动。

---

## 3. 倾斜与反扣状态机（Tilt & FaceDown）

### 3.1 左右倾斜滞后锁定（Tilt Hysteresis）
当用户左右偏转设备（触发 BLE 媒体切歌与宠物侧倾回避反应）时，采用带滞后回差的双门限比较器：
- **人体工学轻触门限**：横滚角 $|\text{Roll}| > 18^\circ$（俯仰角放宽至 $< 50^\circ$），且仅需保持 $\ge 60\text{ms}$：
  - $\text{Roll} < -18^\circ \rightarrow$ 触发左倾斜并进入 `LockedLeft`。
  - $\text{Roll} > +18^\circ \rightarrow$ 触发右倾斜并进入 `LockedRight`。
- **恢复居中门限**：回正至 $|\text{Roll}| < 8^\circ$ 解除锁定，保持防抖回差的同时消除手势顿挫。

### 3.2 屏幕朝下扣放（FaceDown DND）
- **进入免打扰**：当过滤后的重力分量 $G_z < -650\text{mg}$（屏幕朝下反扣在桌面），并稳定保持 $\ge 200\text{ms}$ 时，触发进入反扣状态。UI 立即关闭屏幕背光（0% 亮度），宠物进入深度睡眠 `SleepingZzz`。
- **退出免打扰**：当设备被翻转拾起至 $G_z > 400\text{mg}$，并稳定 $\ge 100\text{ms}$ 时，移除物理反扣原因；白天恢复背光并触发 `StretchAndYawn`，夜间仍按昼夜睡眠策略保持 dim 与睡眠，敲击才申请临时唤醒。
- **可靠方向同步**：上述边沿在 `sensor_task` 交给 `publish_gesture`，转为 `ORIENTATION_SIGNAL<DeviceOrientation>` 的最新权威状态，不占用容量 16 的普通手势队列。队列满不会丢掉唯一的反扣/拾起边沿；应用优先消费方向并幂等更新背光和宠物。短时间多次翻转允许合并，最终状态必须收敛到最新方向，而不是逐条回放过时的睡眠/苏醒动画。

### 3.3 侧向倾斜直接跟随与抬起姿态恒正原则（Lateral Tilt & Pitch-Invariance）
为了实现“抬起不改变，同时往哪一侧倾斜就向那一侧”的人机工学物理交互，`DefaultPet` 与 `InertialTransform` 确立了如下姿态解算规则：
- **抬起不改变原则（Pitch-Invariance on Lift）**：
  将下方向有效重力基准定义为 Y-Z 平面合成重力分量 $G_{\text{down}} = \sqrt{a_y^2 + a_z^2}$。当用户将圆盘从桌面拿起、抬起或立姿斜放面向自己时（仅发生俯仰 Pitch 变化，侧向加速度 $a_x \approx 0$），数码兽始终坚守正向站立姿态（$0^\circ$），彻底消除手持立姿时意外倒置或乱偏的缺陷；
- **侧向倾斜直接跟随（Lateral Side-Tilt Responsiveness）**：
  倾角由侧向重力分量与垂向基准的反正切 $\theta_{\text{tilt}} = \operatorname{fast\_atan2\_deg}(a_x, G_{\text{down}})$ 唯一决定：
  - **向左侧倾斜（$a_x < -60\text{mg}$）**：倾角负向平滑偏转，数码兽向左侧倾斜并自动转身面向左方；
  - **向右侧倾斜（$a_x > +60\text{mg}$）**：倾角正向平滑偏转，数码兽向右侧倾斜并自动转身面向右方；
  - **水平/立姿中位死区（$|a_x| \le 60\text{mg}$）**：消除微小手抖与桌面微平误差，姿态保持 $0^\circ$ 正立。
- **最短圆弧平滑插值（Shortest-Path Circular EMA）**：
  在角度 cross $180^\circ$ 边界时，通过取模 `(target - curr).rem_euclid(360)` 自动选择最短旋转路径，消除跨越对顶角时产生 $360^\circ$ 倒转的视觉突变。
- **倾斜状态自主踱步抑制**：当检测到明显机身侧倾（$|\text{tilt\_deg}| > 8^\circ$）时，主动暂停自主左右踱步（`wander_x`），数码兽集中呈现平衡对抗与侧倾身姿。

---

## 4. 真实物理摇晃与钟摆特训引擎（Shake Engine）

为了完美还原初代数码暴龙机（Digital Monster Pendulum）的物理交互体验，`GestureDetector` 内置了纯定点数（Zero-FPU）的动态摇晃检测状态机：

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> PositivePeak: 动态加速度 >= +260mg
    Idle --> NegativePeak: 动态加速度 <= -260mg
    PositivePeak --> NegativePeak: 40ms <= dt <= 550ms 且反向加速度 <= -180mg (输出 Shake)
    NegativePeak --> PositivePeak: 40ms <= dt <= 550ms 且反向加速度 >= +180mg (输出 Shake)
    PositivePeak --> Idle: dt > 550ms 未反向超时
    NegativePeak --> Idle: dt > 550ms 未反向超时
```

### 4.1 Zero-FPU 8位定点数动态加速度高通提取
在 RV32IMAC 无硬件 FPU 的架构下，重力基准采用 8 位定点数（Q24.8）一阶数字低通滤波追踪：
$$\text{Base}_{k} = \text{Base}_{k-1} + \frac{A_k \ll 8 - \text{Base}_{k-1}}{128}$$
滤波时间常数 $\tau \approx 640\text{ms}$，在准确追踪静态重力倾斜的同时，完全不衰减人类 $2 \sim 5\text{Hz}$ 的自然摇晃动态。动态加速度 $A_{\text{dyn}} = (A_k \ll 8 - \text{Base}_k) \gg 8$。

### 4.2 双向滞后反转门限（Dual-Threshold Peak & Reversal）
- **触发启动门限**：主导轴（X/Y/Z 中动态幅值最大者）绝对值超过 $260\text{mg}$ 进入半波峰值态（`PositivePeak` / `NegativePeak`）。
- **极性反转识别门限**：加速度越过零点反向达到 $180\text{mg}$ 且周期窗满足 $40\text{ms} \le \Delta t \le 550\text{ms}$（对应人类手持晃动频率 $1.8 \sim 12.5\text{Hz}$），确认为一次有效半波摆动，输出 `GestureEvent::Shake` 并切换到对侧波峰等待下次反转。
- **三轴换轴保持**：反转优先沿当前主轴；若跨轴反转，X/Y/Z 三轴均参与候选比较，选择动态幅值最大的反向轴。纯 Z 轴摇晃不会在第一次反转后被错误改写为 X 轴，后续 Z 轴半波仍可连续计数。
- **高频滤除与超时重置**：$< 40\text{ms}$ 判定为机械碰撞冲击或高频杂波予以忽略；$> 550\text{ms}$ 无反向动作则判定为单次位移并平滑回退至 `Idle`。

### 4.3 互斥抑制与防误触保障
- **安全姿态优先**：每个有效样本先推进反扣去抖，再更新摇晃检测；同一样本同时产生反扣边沿与 Shake 时，只输出反扣边沿，不能因为摇晃提前返回而延迟安全状态。
- **敲击与翻页抑制**：X/Y 动态加速度超过 $160\text{mg}$、摇晃半周期进行中或距最后动态运动不足 $500\text{ms}$ 时，复位 tap/tilt 并直接跳过这两项检测，不能在同一样本重新武装。摇晃候选期间的 Z 轴冲击也更新运动冷却时间；孤立、未达到摇晃峰值的 Z 轴敲击不按普通移动屏蔽。受抑制样本仍更新前一原始样本，避免停止后把旧急动度变成延迟 SingleTap。首个样本只建立急动度基线，静止开机不应自动产生敲击。
- **回归边界**：host 回放覆盖反扣与反转同帧、抑制末尾冲击不泄漏 Tap/Tilt、静止开机，以及合法单/双击仍可触发。真实手持波形、阈值和误报率仍需实机测量，不作绝对无误触承诺。
- **桌宠视图自动聚焦**：当检测到 `GestureEvent::Shake` 时，应用层自动将当前视图切回 `ViewMode::Pet`，确保用户能即时观察到数码兽蓄力冲刺与数码矩阵粒子升格进化特效。

---

## 5. 硬件引脚隔离与辅助按键交互（GPIO9 Strapping Pin）

板载侧边按键为 ESP32-C6 的 **BOOT / Strapping 引脚（GPIO9）**：

- **硬件职责隔离**：
  根据硬件规格书说明，GPIO9 在芯片复位期间决定芯片是否进入 ROM 串口下载模式。在桌面圆形硬件形态下，设备日常以手势交互为主（敲击、摇晃、倾斜、反扣）。
- **辅助硬件按键与日常养成导航**：
  `PowerKeyMonitor` 在 200ms 低频安全采样中监测 GPIO9 电平，并在 `AppStateMachine` 中响应按键触发：
  - **按下（Pressed）**：聚焦日常养成，在 `ViewMode::Pet`（桌面主视窗）与 `ViewMode::Status`（数码宝贝全档案卡）之间双向直切，杜绝日常把玩被底层自检数据打扰；
  - **硬件自检透传**：在工程自检界面（`SelfTestView`）实时呈现通断状态（`BOOT:PRSD / REL`）。
