# Digivice: Autonomous Desktop Cyberpet

Coding agents: skip this README and read AGENTS.md directly.

[English](README_EN.md) | [中文](README.md)

> **An Autonomous Desktop Cyberpet & Digivice firmware targeting the Espressif ESP32-C6 SoC in pure `#![no_std]` Rust.**
> Features 6-stage authentic Bandai pixel evolution, Zero-FPU rotated sprite rasterization, and 6-DOF IMU tilt physics.

---

Built for the **Espressif ESP32-C6** (RISC-V RV32IMAC) and the Waveshare ESP32-C6-LCD-1.28 circular development board, **Digivice** is an autonomous desktop cyberpet that operates completely standalone. It tracks motion and orientation, runs local pet interactions and evolution, and periodically saves the current configuration fields without a background daemon or cloud connection. Circadian behavior requires validated hardware time or a calibrated software anchor; an uncalibrated placeholder never schedules nighttime sleep.

Host connectivity (via BLE or USB) purely serves as an enhancement:

- **BLE HID Consumer Control**: Control your computer or smartphone's media playback (Play/Pause, Track Next/Previous, Volume Up/Down, Mute) using physical board gestures.
- **BLE GATT MusicScene**: Synchronize the Digimon's bounce, blink, and animations with your playing music's BPM, energy, and mood.
- **Native USB-Serial-JTAG**: Clean debugging, tracing, and zero-probe firmware flashing over standard Type-C.
- **Autonomous Fallback**: If the wireless link drops or times out, the Digivice smoothly resumes its independent daily routine.

---

## Key Features

### 1. Authentic Bandai Pixel Art & 6-Stage Evolution

Faithfully reimagines the visual aesthetics of the original 1997 Bandai Digivice with high-contrast 16-color pixel art, 2-frame walking animations, and digital matrix evolution VFX:

- **Baby II (幼年期 II)**: Koromon (_16×16_)
- **Child (成长期)**: Agumon (_16×16_)
- **Adult (成熟期)**: Greymon (_24×24_)
- **Perfect (完全体)**: MetalGreymon (_32×32_)
- **Ultimate (究极体)**: WarGreymon (_32×32_)
- **Super Ultimate (超究极体)**: Omegamon (_32×32_)

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

### 4. Local Pet & Presence Companion

- Local interactions and monotonic-time nurture do not require a host connection.
- Battery/charging telemetry and BLE presence can affect presentation and backlight.
- `FocusSession` and focus-history storage are not implemented.
- Software time does not survive power loss by itself; a user-operated time-setting workflow is still pending.

### 5. Periodic, Confirmed Double-Buffered Saves

A single Flash-owning task saves hunger, strength, evolution stage, weight, care-mistake and 16-bit training fields using Schema V3, with read-only V1/V2 compatibility. Requests coalesce and are attempted at least 300 seconds apart, including after failures. With timely scheduling and successful writes, approximately five minutes of uncommitted progress may be lost; persistent I/O failure has no finite loss-window guarantee.

Startup reads only the two configuration records and reports their source, health, and load duration. An incomplete sector read blocks writes with RETRY rather than erasing a potentially valid archive; currently a restart and complete reload are needed to restore writing.

The Status screen reports `SAVE:SYNC/PENDING/WRITING/RETRY #seq`; RAM changes are not called durable before successful verification. Training counts now support 0..65535. V3 restores the age value, sickness, poop, energy, affinity and metabolic remainders without simulating offline physiology. Age advances by positive differences from the last trusted device-calendar day, including offline days; clock rollback never reduces age or recounts days. Untrusted time cannot advance age. Host NOR fault injection checks old-or-new recovery; physical power-cut and flash-endurance measurements remain pending. This is not protection against arbitrary corruption.

---

## Hardware Specifications

| Subsystem      | Specification                   | Notes                                                                                        |
| :------------- | :------------------------------ | :------------------------------------------------------------------------------------------- |
| **SoC**        | Espressif ESP32-C6              | RISC-V 32-bit single-core up to 160MHz                                                       |
| **Display**    | GC9A01 1.28" Circular LCD       | 240×240; SCK=`GPIO11`, MOSI=`GPIO21`, CS=`GPIO10`, DC=`GPIO19`, RST=`GPIO20`                 |
| **Backlight**  | PWM Dimming                     | BL=`GPIO7`                                                                                   |
| **IMU**        | QMI8658 6-Axis Motion Sensor    | I2C address `0x6B`; SCL=`GPIO8`, SDA=`GPIO18`, INT1=`GPIO5`, INT2=`GPIO4`                    |
| **RTC**        | PCF85063ATL Real-Time Clock     | I2C address `0x51`, shared SCL=`GPIO8` / SDA=`GPIO18`; INT=`GPIO6`; optional board component |
| **Wireless**   | 2.4 GHz Radio                   | BLE 5, Wi-Fi 6, 802.15.4; current firmware uses BLE                                          |
| **Storage**    | 16 MB SPI NOR Flash             | Dual-sector Ping-Pong data preservation                                                      |
| **Power**      | USB-C & 3.7V LiPo Battery       | ETA6098; battery ADC=`GPIO0`, charge state=`GPIO23`; 200K/100K divider                       |
| **Micro SD**   | Shared LCD SPI bus              | CS=`GPIO15`, SCK=`GPIO11`, MOSI=`GPIO21`, MISO=`GPIO22`; firmware keeps SD deselected        |
| **BOOT / USB** | Bootstrapping / USB-Serial-JTAG | BOOT=`GPIO9`; USB D-=`GPIO12`, D+=`GPIO13`; do not repurpose these pins                      |

For comprehensive schematic and electrical limits, refer to [`docs/hardware-esp32-c6-lcd-1.28.md`](docs/hardware-esp32-c6-lcd-1.28.md).

---

## Architecture & System Design

The project is structured according to a modern multi-crate Cargo Workspace, separating pure logic, board peripherals, and firmware assembly:

```text
crates/
├── digivice-core/               # Pure logic, zero-hardware models (#![no_std], native host testable)
│   ├── src/
│   │   ├── domain/              # Zero-hardware pure business models (action, clock, imu, music, power)
│   │   ├── drivers/             # Generic sensor drivers (QMI8658 6-axis IMU, PCF85063 RTC)
│   │   ├── gesture/             # Physical motion physics, Zero-FPU filter & gesture detector
│   │   ├── pet/                 # Digital Monster behavior, 16-color assets & animation state machine
│   │   ├── protocol/            # BLE HID Consumer reports, presence tracking & protocol models
│   │   ├── storage/             # Double-buffered ping-pong NOR sector driver, CRC32, blackbox
│   │   └── ui/                  # UiState presentation snapshot & dirty rectangle math
│   └── tests/                   # 5 consolidated high-value test suites (storage, gesture, pet, clock, protocol)
├── digivice-bsp/                # Board Support Package (Waveshare ESP32-C6-LCD-1.28)
│   └── src/
│       ├── display.rs           # GC9A01 LCD driver & backlight PWM
│       ├── power_key.rs         # Battery voltage ADC, charger & BOOT button
│       └── lib.rs               # Hardware bus ownership, pin isolation & Board struct
└── digivice-firmware/           # System firmware binaries and Embassy async runtime
    └── src/
        ├── bin/
        │   ├── digivice.rs      # Cyberpet firmware entry point & task spawner
        │   └── selftest.rs      # Dedicated factory diagnostic firmware
        ├── app/                 # App coordinator & state machine
        ├── ipc/                 # Statically allocated Embassy channels & signals
        ├── radio/               # Trouble-host BLE GATT Server & esp-radio driver
        ├── tasks/               # Embassy async concurrency tasks (app, radio, sensor, storage, ui)
        └── ui/                  # Circular LCD rendering engine (pet_view, status_view, selftest_view)
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
   - The application enforces `#![forbid(unsafe_code)]` in library crates (`digivice-core` and `digivice-firmware`).
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

## License & Intellectual Property Boundaries

### Firmware Source Code

This project firmware source code is dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Graphical Assets & Trademarks

- **Trademark Notice**: Digimon, character designs, and associated names are registered trademarks and copyrights of **BANDAI CO., LTD.**, **WiZ**, and **Toei Animation**. The included dot-matrix sprite assets are intended solely for non-commercial educational study, retro-hardware display experimentation, and personal hobbyist evaluation under fair-use principles. Commercial adaptations must replace all character assets with original artwork.
