# 硬件说明：微雪 ESP32-C6-LCD-1.28

本工程的目标硬件是 **微雪（Waveshare）ESP32-C6-LCD-1.28**，SKU `35450`
（无外壳版；带外壳版为 SKU `35451` 的 `ESP32-C6-LCD-1.28-B`）。

本文基于官方 Wiki、官方原理图以及厂商仓库中的
`HARDWARE_REFERENCE_ZH.md` 交叉整理，用于快速查阅板载资源与引脚占用。

- 官方文档：<https://docs.waveshare.net/ESP32-C6-LCD-1.28/>
- 英文文档：<https://docs.waveshare.com/ESP32-C6-LCD-1.28>
- 原理图：<https://gitee.com/waveshare/esp32-c6-lcd-1.28/raw/main/hardware/schematics/ESP32-C6-LCD-1.28.pdf>
- 官方示例仓库：<https://gitee.com/waveshare/ESP32-C6-LCD-1.28>

## 板卡概览

| 项目 | 参数 |
| --- | --- |
| 主控 | ESP32-C6（RISC-V 单核，最高 160 MHz） |
| 无线 | 2.4 GHz Wi-Fi 6、BLE 5、IEEE 802.15.4（Zigbee / Thread） |
| 内存 | 512 KB HP SRAM、16 KB LP SRAM、320 KB ROM |
| Flash | 16 MB，原理图器件 `XM25QH128DHIQT` |
| 屏幕 | 1.28 英寸圆形 LCD，240 x 240，65K 色，GC9A01，4 线 SPI |
| 传感器 | QMI8658 六轴 IMU（3 轴加速度 + 3 轴陀螺仪） |
| RTC | PCF85063ATL，配 32.768 kHz 晶振 |
| 存储扩展 | Micro SD（TF）卡槽，与 LCD 共用 SPI 总线 |
| 供电 | Type-C、3.7 V MX1.25 锂电池（ETA6098 充放电管理） |
| 充电指示 | 电池充电时常亮，充满熄灭 |
| 扩展 | SH1.0 14PIN（6 路 GPIO、1 路 UART、1 路 I2C） |
| 按键 | BOOT（复位时按下进入下载模式）、RESET |

## 板载外设引脚

| 模块 | 器件 | 接口 | 地址 / 参数 | 引脚 |
| --- | --- | --- | --- | --- |
| LCD | GC9A01 | 4 线 SPI | 240x240，常用 RGB565 | CS=GPIO10，CLK=GPIO11，MOSI=GPIO21，DC=GPIO19，RST=GPIO20，BL=GPIO7 |
| TF 卡 | Micro SD | SPI（与 LCD 共用总线） | - | CS=GPIO15，SCK=GPIO11，MOSI=GPIO21，MISO=GPIO22 |
| IMU | QMI8658 | I2C | 7 位地址 `0x6B` | SCL=GPIO8，SDA=GPIO18，INT1=GPIO5，INT2=GPIO4 |
| RTC | PCF85063ATL | I2C | 7 位地址 `0x51` | SCL=GPIO8，SDA=GPIO18，INT=GPIO6 |
| 电池电压采样 | 分压到 ADC | ADC | R1=200K / R8=100K | GPIO0（BAT_ADC） |
| USB | ESP32-C6 原生 USB-Serial-JTAG | USB | 下载 / 日志 | D-=GPIO12（USB_N），D+=GPIO13（USB_P） |
| UART0 | 调试串口 | UART | 扩展焊盘 | TX=GPIO16，RX=GPIO17 |

要点：**LCD 与 TF 卡共用 SPI**，其中 `CLK=GPIO11`、`MOSI=GPIO21` 为两者共享，
各自有独立片选；TF 卡额外占用 `MISO=GPIO22`。I2C 上 IMU 与 RTC 共享
`SCL=GPIO8` / `SDA=GPIO18`。

## GPIO 占用总表

按 GPIO 编号升序。标注「扩展」的引脚同时引出到 SH1.0 14PIN 接口。

| GPIO | 板上信号 | 说明 |
| ---: | --- | --- |
| GPIO0 | BAT_ADC | 电池电压分压采样（R1=200K / R8=100K） |
| GPIO1 | GPIO1 | 扩展 |
| GPIO2 | GPIO2 | 扩展 |
| GPIO3 | GPIO3 | 扩展 |
| GPIO4 | IMU_INT2 | QMI8658 中断 2（同时引出） |
| GPIO5 | IMU_INT1 | QMI8658 中断 1（同时引出） |
| GPIO6 | RTC_INT | PCF85063 中断（同时引出） |
| GPIO7 | LCD_BL | LCD 背光，可用 PWM 调光 |
| GPIO8 | I2C_SCL | IMU + RTC 共享 I2C 时钟（同时引出） |
| GPIO9 | BOOT | BOOT 按键 / strapping（同时引出） |
| GPIO10 | LCD_CS | GC9A01 片选 |
| GPIO11 | SPI_CLK | LCD + TF 卡共享 SPI 时钟 |
| GPIO12 | USB_N | 原生 USB D- |
| GPIO13 | USB_P | 原生 USB D+ |
| GPIO15 | SD_CS | TF 卡片选 |
| GPIO16 | UART TXD | 调试串口 TX（同时引出） |
| GPIO17 | UART RXD | 调试串口 RX（同时引出） |
| GPIO18 | I2C_SDA | IMU + RTC 共享 I2C 数据（同时引出） |
| GPIO19 | LCD_DC | GC9A01 数据/命令 |
| GPIO20 | LCD_RST | GC9A01 复位 |
| GPIO21 | SPI_MOSI | LCD + TF 卡共享 SPI MOSI |
| GPIO22 | SPI_MISO | TF 卡 SPI MISO（GC9A01 只写，不需要） |
| GPIO23 | BC_State | ETA6098 充电状态（同时引出） |

### 使用注意

- `GPIO12` / `GPIO13` 已接 Type-C 原生 USB，不要当普通 GPIO 使用。
- `GPIO9` 是 BOOT / strapping 引脚，`EN`（CHIP_PU）是复位，均不建议作普通输入。
- QMI8658 / PCF85063 的 I2C 地址分别为 `0x6B` / `0x51`，外挂 I2C 设备时注意
  避免地址冲突。RTC 在原理图中标注为**可选**器件。
- 原理图核对提示：厂商示例代码里有 `BSP_CAPS_TOUCH=1`、
  `BSP_LCD_TOUCH_RST=GPIO23` 等定义，那套 BSP 是为**带触摸的机型**准备的。
  本型号（`ESP32-C6-LCD-1.28`，无触摸）中 `GPIO23` 实际是充电状态 `BC_State`；
  请以原理图和上表为准。

## 官方资料

| 类型 | 链接 |
| --- | --- |
| 中文 Wiki | <https://docs.waveshare.net/ESP32-C6-LCD-1.28/> |
| 英文 Wiki | <https://docs.waveshare.com/ESP32-C6-LCD-1.28> |
| 原理图 PDF | <https://gitee.com/waveshare/esp32-c6-lcd-1.28/raw/main/hardware/schematics/ESP32-C6-LCD-1.28.pdf> |
| 厂商硬件速查 | `HARDWARE_REFERENCE_ZH.md`（官方 Gitee 仓库根目录） |
| 官方示例（ESP-IDF / Arduino） | <https://gitee.com/waveshare/ESP32-C6-LCD-1.28/tree/main/examples> |
| ESP32-C6 数据手册 | <https://documentation.espressif.com/esp32-c6_datasheet_cn.pdf> |
| ESP32-C6 技术参考手册 | <https://documentation.espressif.com/esp32-c6_technical_reference_manual_cn.pdf> |
| GC9A01 手册 | 见官方「相关资料」页 |
