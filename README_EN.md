# Digivice: Autonomous Desktop Cyberpet

[English](README_EN.md) | [中文](README.md)

> **An Autonomous Desktop Cyberpet & Digivice firmware targeting the Espressif ESP32-C6 SoC in pure `#![no_std]` Rust.**
> Features 6-stage authentic Bandai pixel evolution, Zero-FPU rotated sprite rasterization, and 6-DOF IMU tilt physics.

---

Built for the **Espressif ESP32-C6** (RISC-V RV32IMAC) and the Waveshare ESP32-C6-LCD-1.28 circular development board, **Digivice** is an autonomous desktop cyberpet that operates completely standalone. It keeps its own time, tracks motion and orientation, drives evolution cycles, assists your focus sessions, and preserves state across power cycles—all without requiring a background daemon or cloud connection.

Host connectivity (via BLE or USB) purely serves as an enhancement:

- **BLE HID Consumer Control**: Control your computer or smartphone's media playback (Play/Pause, Track Next/Previous, Volume Up/Down, Mute) using physical board gestures.
- **BLE GATT MusicScene**: Synchronize the Digimon's bounce, blink, and animations with your playing music's BPM, energy, and mood.
- **Native USB-Serial-JTAG**: Clean debugging, tracing, and zero-probe firmware flashing over standard Type-C.
- **Autonomous Fallback**: If the wireless link drops or times out, the Digivice smoothly resumes its independent daily routine.

---

## Key Features

### 1. Authentic Bandai Pixel Art & 6-Stage Evolution
Faithfully reimagines the visual aesthetics of the original 1997 Bandai Digivice with high-contrast 16-color pixel art, 2-frame walking animations, and digital matrix evolution VFX:
- **Baby II (幼年期 II)**: Koromon (*16×16*)
- **Child (成长期)**: Agumon (*16×16*)
- **Adult (成熟期)**: Greymon (*24×24*)
- **Perfect (完全体)**: MetalGreymon (*32×32*)
- **Ultimate (究极体)**: WarGreymon (*32×32*)
- **Super Ultimate (超究极体)**: Omegamon (*32×32*)

Evolution progress dynamically accumulates through daily companionship, focused work sessions, and shake training.

### 2. 6-DOF IMU Dynamics & Pitch-Invariant Lateral Tilt
- **Pitch-Invariant Upright Stance**: Lifting or tilting the device towards your face alters the pitch angle ($a_y$ / $a_z$), but keeps the Digimon upright at $0^\circ$ without spurious lateral leaning.
- **True Lateral Tilt Following**: Tilting the hardware to the left or right immediately tilts the Digimon sprite towards that side ($\pm 45^\circ$ clamped), providing delightful physical weight and balance.
- **Pendulum Shake Training**: Rapid physical shaking triggers the classic Digivice pendulum training routine, boosting strength points and battle readiness.
- **Tap & Face-Down Sleep Transitions**: Double-tapping the display cycles modes; turning the board face-down instantly transitions the pet into silent deep sleep.

### 3. RISC-V RV32IMAC Zero-FPU Real-Time Sprite Rasterization
The ESP32-C6 RISC-V core has no hardware floating-point unit (FPU). To achieve 60 FPS animation without trigonometric performance penalties:
- **Precomputed Q12 Sine LUT**: Employs a 91-entry quarter-wave lookup table (`sin_cos_q12`) for high-speed integer trigonometry.
- **Inverse Affine Transformation**: Blits rotated sprites line-by-line directly into streaming GC9A01 screen bands with zero dynamic heap allocation and transparency colorkeying.

### 4. Standalone Focus & Presence Companion
- Built-in `FocusSession` state machine (Focus, Short Break, Long Break, Paused/Completed).
- Real-time battery voltage sensing and ETA6098 charging detection.
- Presence detection: Automatically dims the backlight when you leave the desk, and cheerfully greets you when you return.

### 5. Power-Failure Resilient Double-Buffered Storage
Firmware configuration, pet evolution stages, strength counters, and focus history are persisted via a dual-sector Ping-Pong architecture with monotonic sequence counters (`seq: u32`). Non-atomic writes and flash corruption are mathematically prevented.

---

## Hardware Specifications

| Subsystem | Specification | Notes |
| :--- | :--- | :--- |
| **SoC** | Espressif ESP32-C6 | RISC-V 32-bit single-core up to 160MHz |
| **Display** | GC9A01 1.28" Circular LCD | 240×240 resolution, 4-wire SPI (`GPIO6`, `GPIO7`, `GPIO14`, `GPIO15`, `GPIO22`) |
| **Backlight** | PWM Dimming | `GPIO23`, hardware frequency-controlled |
| **IMU** | QMI8658 6-Axis Motion Sensor | I2C (`GPIO10` SCL, `GPIO11` SDA), accelerometer & gyroscope |
| **RTC** | PCF85063 Real-Time Clock | I2C shared bus, coin-cell backup supported |
| **Wireless** | 2.4 GHz Radio | BLE 5.3, Wi-Fi 6, 802.15.4 (Embassy async `esp-radio`) |
| **Storage** | 16 MB SPI NOR Flash | Dual-sector Ping-Pong data preservation |
| **Power** | USB-C & 3.7V LiPo Battery | ETA6098 PMIC with analog voltage monitoring (`GPIO4`) |

For comprehensive schematic and electrical limits, refer to [`docs/hardware-esp32-c6-lcd-1.28.md`](docs/hardware-esp32-c6-lcd-1.28.md).

---

## Architecture & System Design

The project is structured according to clean Board Support Package (BSP) decoupling and Embassy async concurrency:

```
src/
├── bin/
│   ├── digivice.rs              # Cyberpet runtime entry point and task spawning
│   └── selftest.rs              # Dedicated factory hardware self-test diagnostic tool
├── bsp/                         # Waveshare ESP32-C6-LCD-1.28 hardware abstraction
│   ├── display.rs               # GC9A01 SPI driver & backlight controller
│   ├── imu.rs                   # QMI8658 6-DOF driver & interrupt handling
│   ├── pcf85063.rs              # I2C real-time clock driver
│   ├── power.rs                 # Battery voltage ADC & charging detection
│   └── mod.rs                   # Board peripheral initialization bundle
├── domain/                      # Pure business logic (zero hardware dependencies)
│   ├── digimon.rs               # 6-stage species profiles & evolution requirements
│   ├── focus.rs                 # FocusSession timer & state transitions
│   ├── gesture.rs               # Pitch-invariant lateral tilt & shake detection
│   └── pet.rs                   # Autonomous mood, hunger, and energy models
├── radio/                       # Wireless communication layer
│   ├── ble.rs                   # BLE advertising, SMP Just-Works, and GATT Server
│   └── hid.rs                   # BLE HID Consumer Control media reports
├── storage/                     # Non-volatile data persistence
│   └── flash_state.rs           # Dual-sector Ping-Pong wear-leveling driver
└── ui/                          # Presentation and rendering engine
    ├── assets.rs                # 16-color pixel palettes & Bandai sprite sheets
    ├── circular_layout.rs       # Polar coordinate mapping for round GC9A01 LCD
    ├── sprite.rs                # Zero-FPU rotated affine sprite rasterizer
    └── views.rs                 # Digivice main screen, evolutions, and menus
```

Detailed architectural blueprints:
- [Global Topology & Design Principles](docs/architecture/README.md)
- [Protocol Boundaries: BLE HID & MusicScene](docs/architecture/03-protocol-and-host.md)
- [UI Layout, Pixel Assets & Blit Pipelines](docs/architecture/04-ui-and-graphics.md)
- [Radio Sensing & Wireless Presence](docs/architecture/05-radio-sensing.md)
- [Concurrency, Task Scheduling & Flash Storage](docs/architecture/06-concurrency-and-storage.md)
- [Pet Behavior Contracts & Interaction Models](docs/architecture/07-pet-and-interaction.md)

---

## Development & Building

### Prerequisites

1. Install the Rust bare-metal RISC-V toolchain:
   ```sh
   rustup toolchain install stable --component rust-src --target riscv32imac-unknown-none-elf
   ```
2. Install `espflash` for zero-probe flashing and monitoring:
   ```sh
   cargo install espflash --locked
   ```

### Compile & Static Checks

```sh
# Verify formatting
cargo fmt --all -- --check

# Strict Clippy audit (zero warnings enforced)
cargo clippy --all-features --workspace -- -D warnings

# Build debug binary
cargo build

# Build release binary (recommended for deployment)
cargo build --release
```

### Flashing to Hardware

Connect the ESP32-C6-LCD-1.28 board to your computer via USB-C:

```sh
# Flash and automatically start the serial monitor
cargo run --release

# Or flash directly using espflash
espflash flash --release --monitor
```

---

## Microsoft Rust Training Invariants

This codebase strictly adheres to the **[Microsoft Rust Training](https://microsoft.github.io/RustTraining/)** guidelines for mission-critical embedded systems:

1. **Memory Safety & Zero-Unsafe**:
   - The application enforces `#![forbid(unsafe_code)]` in `src/lib.rs`.
   - Denies `clippy::mem_forget` and `clippy::large_stack_frames`.
2. **Type-Driven Domain Correctness**:
   - Total elimination of Boolean Blindness (`ChargeState`, `ButtonPressState`, `FaceDownTransition`).
   - Elimination of Primitive Obsession via validated domain wrappers (`DateTime::new_clamped`).
3. **Panic-Free Defensive Runtime**:
   - Zero bare `.unwrap()` or `.expect()` in runtime execution loops.
   - All peripheral interactions degrade gracefully without halting the CPU.
4. **Cancellation Safety & Async Ergonomics**:
   - Embassy `select` branches are strictly cancellation-safe.
   - Async read streams yield on idle or EOF to guarantee zero CPU starvation.

---

## License

This project is licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
