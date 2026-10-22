# 07. 可替换宠物与交互扩展边界

本项目的宠物不是写死在应用协调器里的单一角色。`AppStateMachine` 只依赖一份很小的
`PetBehavior` 契约，因此用户可以替换外观、情绪规则和动画，而不需要改 BLE、存储、手势或
显示驱动。

## 1. 分层职责

```text
gesture/      设备级物理输入（敲击、倾斜、FaceDown）
domain/       核心领域数据模型与动作意图（MusicScene, FocusPhase, HidAction）
pet/          PetBehavior、PetEvent 和 PetFrame；不依赖具体渲染器
app/          本地状态协调与工作流（AppStateMachine, FocusSession）
ui/           把 PetFrame 绘制到 LCD；不修改宠物行为
radio/        BLE 在席、GATT 和 HID 传输适配
```

领域模型与动作意图定义在 `domain`，专注工作流状态机位于 `app/focus.rs`：这些独立类型可以被另一种宠物、纯计时 UI 或未来的声音
反馈复用。`pet` 也不属于 `ui`：同一份行为可以输出 LCD、BLE 状态或无屏幕调试快照。

## 2. PetBehavior 契约

源码位置：`src/pet/mod.rs`。

```rust
pub trait PetBehavior {
    fn handle(&mut self, event: PetEvent, now: Instant);
    fn tick(&mut self, now: Instant) -> PetFrame;
}
```

`PetEvent` 是有限、可复制的输入：

- `Tap`、`DoubleTap`、`Shake`：用户互动；
- `FaceDown`、`FaceUp`：安全/睡眠边界；
- `FocusPhase(FocusPhase)`：本地专注节律；
- `MusicScene(MusicScene)`：节拍、能量、情绪和段落；
- `Attitude { ax_mg, ay_mg }`：整数 IMU 姿态。

行为实现只返回 `PetFrame`，其中包含情绪、瞳孔、注视偏移、眨眼、动作计数、能量和亲密度。
这些字段是渲染中立的，不包含 `embedded-graphics` 类型，因此实现可以保持 `no_std`、固定
大小并在没有 LCD 时运行。

## 3. 默认宠物与替换方式

`src/pet/default.rs` 中的 `DefaultPet` 是首个软萌宠物实现，负责：

- 抚摸/双击的短时开心反应和亲密度；
- FaceDown 睡眠、FaceUp 唤醒；
- 专注时的稳定眼神；
- 音乐场景的低优先级情绪反应；
- 基于整数 LUT 的眨眼和 IMU 注视平滑。

替换宠物只需在固件入口构造实现了 `PetBehavior` 的类型：

```rust
struct MyPet { /* only fixed-size state */ }

impl PetBehavior for MyPet {
    fn handle(&mut self, event: PetEvent, now: Instant) { /* custom rules */ }
    fn tick(&mut self, now: Instant) -> PetFrame { /* custom frame */ }
}

let app = AppStateMachine::with_pet(config, MyPet::new());
```

应用协调器通过泛型静态分发接入自定义宠物，不使用堆分配或运行时反射。若未来需要从
Flash 选择角色，可以在编译期用枚举包装多个 `PetBehavior` 实现，再由配置选择枚举分支；
不能把不受控的脚本解释器放入实时任务。

## 4. 交互扩展规则

新增领域模型与工作流先放入 `domain/` 或 `app/` 下职责明确的模块，仅在确实需要硬件输入时才修改 `gesture/` 或 `radio/`：

1. 为输入定义带校验的类型，而不是裸 `bool` 或魔法数字；
2. 在 `PetEvent` 中加入语义事件，避免把 BLE/GATT 细节泄漏到宠物实现；
3. 在 `UiState` 中只保存可复制的最新快照，不排队历史动画帧；
4. 更新本文件、并发拓扑和对应协议文档后再修改协调器；
5. 断开 BLE、拔掉 USB 或切换宠物实现，都不能停止本地 FocusSession 和动画时钟。

## 5. 验收标准

- `AppStateMachine<DefaultPet>` 与 `AppStateMachine<CustomPet>` 共享同一套手势、BLE、存储和
  UI 代码；
- `pet/` 不导入 `embedded-graphics`、`radio` 或 `board` 驱动；
- 自定义宠物能在 `no_std` 下编译，且不引入无界容器、运行时 `unwrap` 或高频浮点三角函数；
- `PetFrame` 变化只在宠物视图需要时标记 UI 脏，不会让 SelfTest 视图被动画刷新拖慢；
- 音乐特征超时、FaceDown 和 BLE 断连均能回到本地安全状态。
