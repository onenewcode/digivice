# 07. 可替换宠物与交互扩展边界

`AppStateMachine` 依赖小而稳定的 `PetBehavior` 契约，因此可以替换外观、情绪规则和动画，
而不需要让宠物实现了解 BLE、Flash、手势驱动或 LCD 驱动的细节。当前产品行为由 `app/`、
`domain/` 和 `pet/` 共同组成。

## 1. 分层职责

```text
gesture/      设备级物理输入（敲击、倾斜、FaceDown、旋转）
domain/       核心领域模型（MusicScene, HidAction, ChargeState）
pet/          PetBehavior、PetEvent、PetFrame、PetAction、PetSpecies；不依赖传输或渲染
app/          AppStateMachine，本地状态协调
ui/           把 UiState/PetFrame 绘制到 LCD
radio/        BLE 在席、GATT 和 HID 传输适配
storage/      DeviceConfig、CRC、双扇区和黑匣子存储
```

领域类型可以被不同宠物实现或无屏幕测试复用。`pet/` 不得导入
`embedded-graphics`、`radio` 或 `board`，渲染器也不能反向修改宠物的可变状态。

## 2. `PetBehavior` 契约与领域模型

源码位置：`crates/digivice-core/src/pet/mod.rs`。

```rust
pub trait PetBehavior {
    fn handle(&mut self, event: PetEvent, now: Instant);
    fn tick(&mut self, now: Instant) -> PetFrame;
    fn archive(&self) -> Option<PetArchive> { None }
}
```

`PetEvent` 是固定大小且可复制的输入：

- `Tap`、`DoubleTap`、`Shake`、`Attack`：本地互动、投喂、钟摆特训与必杀释放；
- `FaceDown`、`FaceUp`：屏幕翻转睡眠与唤醒；
- `MusicScene(MusicScene)`：节拍、能量、情绪和段落；
- `Attitude { ax_mg, ay_mg, az_mg }`：三轴重力加速度与侧倾姿态解算输入；
- `ChargeState(ChargeState)`：USB 供电与锂电池充电状态；
- `CircadianHour(u8)`：仅可信硬件/已锚定外推时间产生的小时边沿，驱动作息（22:00~07:00 睡眠）；未设置时间不驱动昼夜。
- `CircadianUnavailable`：只清除昼夜阶段及临时授权，保留物理反扣原因和最后可信日锚点。
- `CalendarDay(CalendarDay)`：仅可信日历/外推产生的设备日历日，独立于小时边沿结算年龄。

### 2.1 物种、模块化资产与动作体系 (`crates/digivice-core/src/pet/species.rs` & `crates/digivice-core/src/pet/assets/`)

系统采用数据驱动与强类型设计，彻底消除布尔盲区与基元执念：

- **模块化美术资产目录 (`crates/digivice-core/src/pet/assets/`)**：
  - 资产严格按物种名称分模块独立存放：
    - `assets/koromon.rs`：滚球兽（幼年期 II）32x24 经典泡泡糖粉圆身躯、红宝石大眼、纯白小乳牙与下垂兔耳；
    - `assets/agumon.rs`：亚古兽（成长期）32x28 经典正统双足兽脚类恐龙身躯、箱型方吻、双侧绿宝石大眼、前伸双手各有3根尖锐白爪、双足3爪踏地与平衡恐龙尾；
    - `assets/greymon.rs`：暴龙兽（成熟期）36x28 强壮暴龙身躯配深蓝闪电虎斑纹、三叉角褐色骨盔、上下颚巨兽尖牙与超级火焰（Mega Flame）；
    - `assets/metal_greymon.rs`：机械暴龙兽（完全体）38x30 半机械半肉改造型暴龙、左臂银铬合金三叉戟机械爪（Trident Arm）、破损撕裂紫蓝龙翼、胸部导弹舱（Giga Destroyer）；
    - `assets/war_greymon.rs`：战斗暴龙兽（究极体）34x32 龙人型战士、金色时之数码合金全身战甲、双臂装备三联恐龙克星（Dramon Killer）龙爪巨刃、背部折叠勇气之盾与飘逸绯红长发；
    - `assets/omegamon.rs`：奥米加兽（超究极体）36x34 皇家圣骑士修长纯白圣甲、胸口苍蓝宝石、右臂钢铁加鲁鲁狼首（Garuru Cannon）、左臂战斗暴龙龙首（Grey Sword）、双色骑士披风与绝对零度冷冻炮（Supreme Cannon）；
    - `assets/mod.rs`：统一 16 色调色板 (`DIGIMON_16_PALETTE`) 与通用交互道具（烤肉、便便、心心、音符）；
  - **主流正统数码宝贝造型与共享调色板 (`DIGIMON_16_PALETTE`)**：
    - 统一 16 色 RGB565 色表，色值 0 为透明穿透色，完美覆盖 6 个进化阶段所有体色、金属铠甲与特效；
    - 全物种资产总 Flash 空间消耗仅 **8.4 KB**，占板载 16MB Flash 的不到 0.05%；
  - **解剖学分层像素切片与视口放大**：
    - 3x 像素视口渲染（主体 96~114px，占圆屏直径 40%~50%），消除过小感；
    - 统一足底接地板基准（`floor_y: 164`）：全物种无论站立还是下蹲，足底稳固贴合赛博底座，产生真实弹性形变而不浮空；
  - **数码矩阵升格进化特效 (Digital Matrix VFX)**：
    - 在 `PetAction::Evolving { progress_pct, target_species }` 期间，触发 6 道纵向升格数码粒子光柱；
    - 颜色沿青绿科技光（`0x06FD`）、数码合金金（`0xFEC0`）、纯白高光（`0xFFFF`）和天蓝等离子（`0x5D3F`）梯次律动，进度达到 100% 后平滑切入新物种形态并触发 `HappyHop` 庆祝。
- **物种目录与生命周期 (`PetSpecies`)**：
  - 完整支持八神太一主线 6 阶段数码宝贝进化：`Koromon` (0: In-Training II) -> `Agumon` (1: Rookie) -> `Greymon` (2: Champion) -> `MetalGreymon` (3: Ultimate) -> `WarGreymon` (4: Mega) -> `Omegamon` (5: Ultra)；
  - 提供纯英文命名与阶位访问器：`stage_index(&self) -> u8`、`from_stage(u8) -> PetSpecies`、`name(&self) -> &'static str`、`tier_name(&self) -> &'static str`、`next_evolution(&self) -> PetSpecies`、`has_next_evolution(&self) -> bool`；
  - 源码严格遵循无中文描述规范，所有方法与契约采用标准英文命名与注释；
  - 钟摆特训进阶：持续摇动达到 10 次完成一轮特训，力量充满并触发数码矩阵进化演变。
- **三段式动作生命周期 (`ActionPhase`) 与有界仲裁队列**：
  - `Enter`：预备起势帧（下蹲蓄力、发现食物愣住）；
  - `Loop`：核心循环帧（持续弹跳、咀嚼、甩耳、数码能量充能）；
  - `Exit`：优雅收尾帧（落地缓冲回弹、擦嘴收工、收耳立正）。
  - **动作优先级 (`ActionPriority`)**：`Low`（闲置/漫步/跳舞）、`Normal`（摸头/伸懒腰/摇摆特训）、`High`（喂食/排泄清理/治病/必杀特技）、`Critical`（进化序列/睡眠）；
  - **请求与排队契约 (`QueuedAction`)**：动作请求携带独立 `duration_ticks` 与优先级；使用 4 槽静态环形队列 `Deque<QueuedAction, 4>` 保留请求，队列满时根据优先级淘汰低优先级项，杜绝单槽覆盖丢失时长；
  - **不可中断与平滑过渡**：进食、治疗、清理、必杀与进化序列具备不可中断保证（`is_uninterruptible`），交互事件顺序排队并按 Enter->Loop->Exit 完整生命周期出队执行；可中断动作被同等或更高优先级请求打断时，先执行 2 帧 Exit 平滑收尾再进入新动作 Enter；安全睡眠（FaceDown/夜间）拥有最高优先级可立即中断任何动作并清空队列；
  - **漫游守卫**：自主拓麻歌子漫步仅在宠物处于待机呼吸且动作队列为空时推进，不会意外打断或覆盖正在执行的交互动作。
- **细分动作词汇 (`PetAction`)**：包含待机呼吸、张望、伸展打哈欠、摸头跳跃、进食咀嚼、专注跳跃特训、蓝牙音乐律动舞蹈（`MusicDance` 归属宠物自身）、侧倾平衡、旋钮转向（`StepLeg` 步态）、离桌悬空、睡眠、进化序列（`Evolving { progress_pct, target_species }`）与必杀技能释放（`AttackSpecial { tick }`）；
- **强类型状态**：`SleepState`（`Awake` / `Asleep`）与 `ChargeState`（`StandbyOrDone` / `Charging`）。

### 2.2 平滑过渡与零浮点弹性物理 (`crates/digivice-core/src/pet/transition.rs`)

- **挤压与拉伸 (`SquashStretch`)**：利用预计算的 8 阶弹簧阻尼表 `SPRING_SQUASH_LUT` 和安全访问器 `spring_squash_step`，实现软体果冻弹性体积守恒补偿；
- **整数惯性阻尼 (`InertialTransform`)**：位置按 `old + (target - old) / 4` 推进，倾角先求最短圆弧差再按相同比例推进；误差绝对值小于 4 时直接吸附到目标，避免整数截断造成永久残留。位置差和中间计算拓宽至 `i32`，保持正负方向对称且不越过目标；倾角输出归一化至 `(-180, 180]`，恰好相差 180° 时沿正方向推进。全程无浮点运算，不增加状态或堆分配。

**平滑过渡回归边界**：`crates/digivice-core/src/pet/transition.rs` 内置 host 单元测试，直接编译实际运动逻辑，不引入 BSP、Embassy 或固件依赖。覆盖小位置误差、`i16` 极值、不越界/不越过目标、全部 `i16` 初始倾角归零、归一化角度对的单调收敛及双向跨 ±180° 最短圆弧；由 `scripts/test-host.sh` 及 CI 的 `host-tests` 自动化回归执行。

## 3. 宠物自主状态机与拓麻歌子漫步系统

### 3.1 睡眠来源与临时唤醒

`pet/sleep.rs` 独立保存物理方向、昼夜阶段（未知/白天/夜间）和单调时间临时唤醒截止点，不再让 RTC 小时边沿调用物理反扣/拾起处理：

- 反扣优先于一切普通交互；白天整点或跨 07:00 不能唤醒仍反扣的宠物。
- 未知时间不安排夜间睡眠；只有可信时间能够建立昼夜阶段，未知不能取消反扣安全状态。
- 22:00～07:00 为夜间；夜间拾起设备只移除物理反扣原因，不自动取消昼夜睡眠。
- 夜间正面朝上时单击/双击可临时唤醒 30 秒；首次敲击只唤醒、不同时投喂。继续敲击可续期；截止时间由 `tick(now)` 检查，不依赖 RTC 秒钟或维护次数。
- 再次反扣会清除临时唤醒授权，之后夜间拾起仍睡眠；跨夜间小时不会错误清除尚未过期的授权。
- 夜间睡眠背光最多 12%，且不高于配置的正常亮度；物理反扣背光始终为 0。UI 睡眠绘制最多 2Hz，唤醒后不受睡眠帧截止点限制。应用仍需维护时间和安全状态，不称为 MCU 深度睡眠。

host 回放直接编译完整 `pet` 模块，测试套件覆盖反扣跨黎明/白天整点、夜间拾起、夜间敲击超时与物理反扣优先级的全组合逻辑，并在 `scripts/test-host.sh` 中常态化验证。

### 3.2 单调时间养成时基

`pet/nurture.rs` 用 `Instant`/`Duration` 维护实际清醒时长，与动画 tick、显示帧率和 RTC 日历边沿分离。当前明确规则为：清醒每 30 秒消化一次；饥饿连续 60 秒或便便不少于 3 坨且连续 40 秒未清理会生病；持续呼叫每满 180 秒记录一次照料失误。动画仍可按维护节拍推进，但不得再用 `1200`、`1800`、`5400` 次调用表达生理时间。

- 睡眠暂停消化、疾病和照料累计，不清零已累计的清醒进度；不在睡眠时继续生成便便。恢复后继续累计，而非借反扣重置照料窗口。
- 所有 `PetBehavior::handle(event, now)` 先结算事件发生前的经过时间，再应用投喂、清理或睡眠事件，避免调度停顿后把过去时间错误归给新状态。
- 临时夜间唤醒跨长停顿时，先结算到 30 秒授权截止点，再切换睡眠，之后的睡眠时间不算作清醒。
- 长跳步最多逐个处理 7 个消化边界；饥饿/排泄进入有界固定点后，剩余时间用商和余数合并处理。无按丢失 tick 数补跑的无界循环，计数饱和且不对任意大时间直接相加。
- 过时的单调时间戳不回退锚点，不重复结算。跨重启规则明确：代谢进度恢复但断电期间不补算生理状态，年龄按可信设备日历日差结算，不做断电期间的饥饿/疾病/照料模拟。

host 测试套件覆盖 20/30/40Hz、稀疏时间戳与 600 秒长停顿的一致性，精确的 180 秒照料边界，60/40 秒疾病边界，睡眠暂停/恢复、停顿后的投喂因果顺序、夜间授权截止及 `u64::MAX` 时间跳步。

### 3.3 自主行为

`crates/digivice-core/src/pet/default.rs` 实现正统数码兽/拓麻歌子自主行为：

- **自主巡游漫步系统 (Autonomous Life Wandering Cycle)**：
  - 宠物在空闲无外部事件时，会自主在接地板基座上左右踱步探索（`wander_x: -30..=30`），到达目标后随机驻足四处张望（LookAround/Screen Peek 撒娇贴屏）；
  - 踱步期间与 `StepLeg` 深度联动，呈现经典 2 帧交替摇摆（左脚/右脚踏步并产生 2px 垂直微弹动，Frame 0 与 Frame 1 动态切帧）；
  - 精灵朝向依据移动方向实时翻转（`SpriteFlip::Normal` / `SpriteFlip::Flipped`），拒绝呆板单向滑动。
- **全物理手势交互与微表情**：
  - 结合 IMU 重力姿态、倾斜平衡、摇晃计数与敲击手势；
  - 蓝牙音乐遥测作为第一公民动作，依据 BPM 周期与节拍相位精准驱动 5 级编舞（带专属霓虹 EQ 与耳机特效）；
  - 状态机在事件到来时保证动作优雅收尾（Exit）再切入，杜绝粗暴跳帧与瞬移；
  - 维护 RTC 昼夜节律与体征代谢（饱腹心形与力量闪电指示、吃肉、如厕排泄、Zzz 睡眠气泡、真实体重与年龄增长）；
  - **经典呼叫提醒与失误系统 (Attention Call & Care Mistakes)**：当数码兽饱腹度归零、地面有便便未清或生病时，右上角经典像素呼叫灯（⚠️）闪烁提醒；超时 3 分钟未予照料则记录 1 次养育失误（`care_mistakes`）；
  - **生病与抚摸治疗机制 (Sickness & Healing)**：便便堆积 $\ge 3$ 坨或饥饿维持过久，数码兽触发生病状态（骷髅 💀 特效、虚弱眩晕），轻拍或照料触发治愈包扎（`HealCure`）恢复健康。

## 4. 应用交互映射

当前 `AppStateMachine` 的正统数码机养成交互映射如下：

| 输入 | 场景条件 | 本地数码兽养成行为 | 可选 BLE HID 动作 |
| :--- | :--- | :--- | :--- |
| `SingleTap` | 生病状态 (`is_sick`) | **治愈疾病 (HealCure)**，包扎创口贴特效，好感 $+15$，恢复健康 | `PlayPause` |
| `SingleTap` | 健康日常状态 | 抚摸数码兽，好感度 $+5$，触发 `HappyHop` 与爱心特效 | `PlayPause` |
| `DoubleTap` | 生病状态 (`is_sick`) | **治愈疾病 (HealCure)**，恢复健康与心情 | `NextTrack` |
| `DoubleTap` | 饥饿度 $< 4$ | **投喂烤肉 (Roast Meat 🍖)**，饱腹 $+1$、体重 $+1\text{g}$、能量 $+20$、好感 $+10$ | `NextTrack` |
| `DoubleTap` | 饱腹 $= 4$ 且力量 $< 4$ | **投喂超级维生素胶囊 (Protein Pill 💊)**，力量 $+1$、体重 $+2\text{g}$、能量 $+30$、好感 $+15$ | `NextTrack` |
| `DoubleTap` | 饱腹与力量双满 ($4/4$) | **战意昂扬，释放专属必杀技 `AttackSpecial`** | `NextTrack` |
| `TiltLeft` / `TiltRight` | 地面有便便 (`poop > 0`) | **冲水清扫 (Toilet Flush 🚽)**：数字科技水浪横扫接地板冲净便便，数码兽解脱欢跳 `HappyHop`，好感 $+10$ | 无 (纯本地照料清洁) |
| `TiltLeft` | 地面无便便 | 机身侧倾平衡反应，消除误喂副作用 | `PreviousTrack` |
| `TiltRight` | 地面无便便 | 机身侧倾平衡反应，消除误喂副作用 | `NextTrack` |
| `Shake` (1..9次) | 3秒内连续摆动 | **钟摆特训蓄力**，触发 `ShakeChargeSprint` 并实时刷新 10 段式计量槽 | 无 |
| `Shake` (第10次) | 特训达标 | 力量填满 4 格、能量 100%、特训次数 $+1$、燃烧体重 $-1\text{g}$（不低于物种基准）；可进化时触发数码矩阵升格，终阶释放究极必杀 | 状态合并到待保存配置；按 300 秒限频提交，确认前仍为 PENDING/WRITING |
| `FaceDownEnter` | 反扣桌面 | 背光 0% 熄灭，数码兽进入 `SleepingZzz` 深度睡眠 | 无 |
| `FaceDownExit` | 拾起正面朝上 | 恢复背光，数码兽触发 `StretchAndYawn` 伸展苏醒 | 无 |
| `BOOT` (短按) | 日常视窗 | **日常养成直切**：在 `Pet`（宠物主视窗）与 `Status`（数码宝贝全档案卡）之间双向切换 | 无 |

HID 发送通过 `HID_ACTION_CHANNEL` 完成；宠物行为本身不能直接发送 BLE 报告。没有 BLE 客户端
时，HID 动作被丢弃，本地宠物状态仍然更新。

## 5. 替换方式与扩展规则

应用层提供泛型构造函数 `AppStateMachine::with_pet(config, pet)`，自定义实现只需保持固定大小，实现 `PetBehavior` 契约即可。

## 6. 跨重启养成恢复

`domain::PetArchive` 是验证后的固定大小存档值，`PetArchiveData` 仅用于字段传递，必须经 clamped 构造才能应用到宠物。它覆盖阶段、饥饿/力量、体重、照料失误、u16 训练/年龄、健康、排泄、能量、亲密度、代谢已累计进度和日历日锚点，不包含 `PetFrame` 的表情、动画、半途特训或实时睡眠来源。自定义宠物可覆盖默认返回 None 的 `PetBehavior::archive`；只提供渲染帧的实现仍可工作，但不能据此宣称自定义内部时基已完整持久化。

默认宠物通过 `from_archive(archive, now)` 恢复属性，按新的单调 `now` 重锚计时器；不会带入旧启动 Instant，也不把断电时长补算成清醒时间。睡眠/方向/充电重新服从当前传感器与可信时钟，30 秒临时唤醒、未完成摇晃和动作队列不恢复。恢复时重新推导呼叫状态，清除不适用的条件累计，并为已病宠物建立正确病态表情。

`NurtureTimers::snapshot` 只导出有界累计毫秒（小于 1ms 的分数向下取整），周期/疾病上限与领域进度共享唯一常量。恢复保留消化、饥饿致病、未清理致病和照料呼叫余量；不能通过重启清零 180 秒失误或 60/40 秒疾病窗口。年龄显示与存档统一为 u16；年龄通过可信日历日差更新，与昼夜小时边沿和代谢时间分别结算。

应用启动通过 `DeviceConfig::pet_archive` 构造默认宠物；维护后 `current_config` 优先读取宠物真实 archive，并通过 `set_pet_archive` 映射回配置，不再用 UI 帧替代私有生理计时器。host 使用相同 codec/桥接/默认宠物恢复完整字段，覆盖新旧单调起点、断电不补算、各条件剩余阈值、临时睡眠不恢复；还通过真实 NOR owner 保存实际发生互动后的宠物并重新构造，验证病态、排泄、亲密度、300 次训练和非零余量。实机重启/掉电和完整 UI 像素仍需验收。

### 6.1 可信设备日历增龄与回退规则

`ClockSnapshot::trusted_calendar_day` 仅在 HardwarePresent 或已锚定 FallbackSoftware 时产生 `CalendarDay`；应用把该值交给宠物，不从占位日期造锚点。第一次可信日期只建立基准、不凭空加岁。之后新日与上次已计入日的正向差增加年龄，因此同小时跨多日、长停机、闰日/跨年与连续逐日通知等价；用一次饱和加法处理，不补跑逐日循环。

年龄与日锚点一起持久化。时间向后调整时保留原锚点，不减龄、不重复计算旧日期；再次超过原锚点才继续增龄。失去时钟可信度只暂停日历更新，不擦掉锚点。大幅向前校时会按正向日差增加年龄，向后校时不撤销已计年龄；本规则不是对 RTC 提供的合法但错误日期的真实性证明，也不做时区/DST 转换。无可信时间的设备保留已存年龄，不能宣称具备真实离线增龄。

回放测试覆盖首次可信时间、2020～2099 全部逐日与首尾稀疏更新等价、平闰年/跨年、重复/回退、连续三次重启不重复补龄、离线不补生理、不可信日期隔离和年龄饱和，NOR 宠物提交/恢复回归同时保存实际日锚点。
