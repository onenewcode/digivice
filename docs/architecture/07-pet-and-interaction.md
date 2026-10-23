# 07. 可替换宠物与交互扩展边界

`AppStateMachine` 依赖小而稳定的 `PetBehavior` 契约，因此可以替换外观、情绪规则和动画，
而不需要让宠物实现了解 BLE、Flash、手势驱动或 LCD 驱动的细节。当前产品行为由 `app/`、
`domain/` 和 `pet/` 共同组成。

## 1. 分层职责

```text
gesture/      设备级物理输入（敲击、倾斜、FaceDown、旋转）
domain/       核心领域模型（MusicScene, FocusPhase, HidAction, ChargeState）
pet/          PetBehavior、PetEvent、PetFrame、PetAction、PetSpecies；不依赖传输或渲染
app/          AppStateMachine 与 FocusSession，本地状态协调
ui/           把 UiState/PetFrame 绘制到 LCD
radio/        BLE 在席、GATT 和 HID 传输适配
storage/      DeviceConfig、CRC、双扇区和黑匣子存储
```

领域类型和 `FocusSession` 可以被不同宠物实现或无屏幕测试复用。`pet/` 不得导入
`embedded-graphics`、`radio` 或 `board`，渲染器也不能反向修改宠物的可变状态。

## 2. `PetBehavior` 契约与领域模型

源码位置：`src/pet/mod.rs`。

```rust
pub trait PetBehavior {
    fn handle(&mut self, event: PetEvent, now: Instant);
    fn tick(&mut self, now: Instant) -> PetFrame;
}
```

`PetEvent` 是固定大小且可复制的输入：

- `Tap`、`DoubleTap`、`Shake`：本地互动、投喂与钟摆特训；
- `FaceDown`、`FaceUp`：屏幕翻转睡眠与唤醒；
- `FocusPhase(FocusPhase)`：本地专注节律；
- `MusicScene(MusicScene)`：节拍、能量、情绪和段落；
- `Attitude { ax_mg, ay_mg, az_mg }`：三轴重力加速度与侧倾姿态解算输入；
- `ChargeState(ChargeState)`：USB 供电与锂电池充电状态；
- `CircadianHour(u8)`：RTC 边沿触发真实昼夜代谢与作息（22:00~07:00 睡眠）。

### 2.1 物种、模块化资产与动作体系 (`src/pet/species.rs` & `src/pet/assets/`)

系统采用数据驱动与强类型设计，彻底消除布尔盲区与基元执念：

- **模块化美术资产目录 (`src/pet/assets/`)**：
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
- **三段式动作生命周期 (`ActionPhase`)**：
  - `Enter`：预备起势帧（下蹲蓄力、发现食物愣住）；
  - `Loop`：核心循环帧（持续弹跳、咀嚼、甩耳、数码能量充能）；
  - `Exit`：优雅收尾帧（落地缓冲回弹、擦嘴收工、收耳立正）。
- **细分动作词汇 (`PetAction`)**：包含待机呼吸、张望、伸展打哈欠、摸头跳跃、进食咀嚼、专注跳跃特训、蓝牙音乐律动舞蹈（`MusicDance` 归属宠物自身）、侧倾平衡、旋钮转向（`StepLeg` 步态）、离桌悬空、睡眠、进化序列（`Evolving { progress_pct, target_species }`）与必杀技能释放（`AttackSpecial { tick }`）；
- **强类型状态**：`SleepState`（`Awake` / `Asleep`）与 `ChargeState`（`StandbyOrDone` / `Charging`）。

### 2.2 平滑过渡与零浮点弹性物理 (`src/pet/transition.rs`)

- **挤压与拉伸 (`SquashStretch`)**：利用预计算的 8 阶弹簧阻尼表 `SPRING_SQUASH_LUT` 和安全访问器 `spring_squash_step`，实现软体果冻弹性体积守恒补偿；
- **整数惯性阻尼 (`InertialTransform`)**：采用 `(old * 3 + target) / 4` 的定点数指数移动平均（EMA），中间计算拓宽至 `i32` 防止算术溢出，实现无浮点、无瞬移的平滑位移滑移。

## 3. 宠物自主状态机与拓麻歌子漫步系统

`src/pet/default.rs` 实现正统数码兽/拓麻歌子自主行为：

- **自主巡游漫步系统 (Autonomous Life Wandering Cycle)**：
  - 宠物在空闲无外部事件时，会自主在接地板基座上左右踱步探索（`wander_x: -30..=30`），到达目标后随机驻足四处张望（LookAround/Screen Peek 撒娇贴屏）；
  - 踱步期间与 `StepLeg` 深度联动，呈现经典 2 帧交替摇摆（左脚/右脚踏步并产生 2px 垂直微弹动，Frame 0 与 Frame 1 动态切帧）；
  - 精灵朝向依据移动方向实时翻转（`SpriteFlip::Normal` / `SpriteFlip::Flipped`），拒绝呆板单向滑动。
- **全物理手势交互与微表情**：
  - 结合 IMU 重力姿态、倾斜平衡、摇晃计数与敲击手势；
  - 蓝牙音乐遥测作为第一公民动作，依据 BPM 周期与节拍相位精准驱动 5 级编舞（带专属霓虹 EQ 与耳机特效）；
  - 状态机在事件到来时保证动作优雅收尾（Exit）再切入，杜绝粗暴跳帧与瞬移；
  - 维护 RTC 昼夜节律与体征代谢（饱腹心形与力量闪电指示、吃肉、如厕排泄、Zzz 睡眠气泡）。

## 4. 应用交互映射

当前 `AppStateMachine` 的交互映射如下：

| 输入            | 本地宠物/专注行为             | 可选 BLE HID 动作 |
| --------------- | ----------------------------- | ----------------- |
| `SingleTap`     | 抚摸宠物并切换 `FocusSession` | `PlayPause`       |
| `DoubleTap`     | 奖励宠物、提升能量和亲密度    | `NextTrack`       |
| `TiltLeft`      | 触发宠物短反应                | `PreviousTrack`   |
| `TiltRight`     | 触发宠物短反应                | `NextTrack`       |
| `Shake`         | 钟摆特训 (摇动10次蓄满力量并触发数码矩阵进化) | 无                |
| `FaceDownEnter` | 暂停专注并进入睡眠            | 无                |
| `FaceDownExit`  | 恢复专注剩余时间并唤醒        | 无                |

HID 发送通过 `HID_ACTION_CHANNEL` 完成；宠物行为本身不能直接发送 BLE 报告。没有 BLE 客户端
时，HID 动作被丢弃，本地宠物和专注状态仍然更新。

## 5. 替换方式与扩展规则

应用层提供泛型构造函数 `AppStateMachine::with_pet(config, pet)`，自定义实现只需保持固定大小，实现 `PetBehavior` 契约即可。
