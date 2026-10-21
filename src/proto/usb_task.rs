//! USB-Serial-JTAG Async Link Task & Host Watchdog

pub use super::DeviceEvent;
use super::{HostPacket, parse_line};
use core::fmt::Write as _;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embedded_io_async::{Read, Write};
use esp_hal::Async;
use esp_hal::usb::usb_serial_jtag::{UsbSerialJtagRx, UsbSerialJtagTx};
use heapless::String;

pub type HostPacketChannel = Channel<CriticalSectionRawMutex, HostPacket, 16>;
pub type DeviceEventChannel = Channel<CriticalSectionRawMutex, DeviceEvent, 8>;

pub static HOST_PACKET_CHANNEL: HostPacketChannel = Channel::new();
pub static DEVICE_EVENT_CHANNEL: DeviceEventChannel = Channel::new();

pub struct UsbRxLink<'d> {
    rx: UsbSerialJtagRx<'d, Async>,
    line_buf: [u8; 256],
    line_len: usize,
}

impl<'d> UsbRxLink<'d> {
    pub fn new(rx: UsbSerialJtagRx<'d, Async>) -> Self {
        Self {
            rx,
            line_buf: [0u8; 256],
            line_len: 0,
        }
    }

    /// Read incoming bytes and push parsed HostPacket to HOST_PACKET_CHANNEL
    pub async fn process_rx_chunk(&mut self) {
        let mut temp_buf = [0u8; 64];
        let n: usize = self.rx.read(&mut temp_buf).await.unwrap_or_default();
        if n == 0 {
            // Prevent hot-loop spin on EOF or disconnected USB-Serial-JTAG
            embassy_time::Timer::after(embassy_time::Duration::from_millis(20)).await;
            return;
        }
        for &byte in &temp_buf[..n] {
            if byte == b'\n' || byte == b'\r' {
                if self.line_len > 0 {
                    if let Ok(line_str) = core::str::from_utf8(&self.line_buf[..self.line_len])
                        && let Ok(packet) = parse_line(line_str)
                    {
                        HOST_PACKET_CHANNEL.send(packet).await;
                    }
                    self.line_len = 0;
                }
                // If line_len == 0, ignore trailing delimiters (e.g. \n following \r)
            } else if self.line_len < self.line_buf.len() {
                self.line_buf[self.line_len] = byte;
                self.line_len += 1;
            } else {
                // Overflow: reset buffer
                self.line_len = 0;
            }
        }
    }
}

pub struct UsbTxLink<'d> {
    tx: UsbSerialJtagTx<'d, Async>,
}

impl<'d> UsbTxLink<'d> {
    pub fn new(tx: UsbSerialJtagTx<'d, Async>) -> Self {
        Self { tx }
    }

    /// Send formatted string over USB
    pub async fn send_str(&mut self, s: &str) {
        let _ = self.tx.write_all(s.as_bytes()).await;
    }

    /// Send DeviceEvent over USB with wire newline framing
    pub async fn send_event(&mut self, evt: &DeviceEvent) {
        let mut msg = String::<96>::new();
        if writeln!(msg, "{evt}").is_ok() {
            self.send_str(&msg).await;
        }
    }
}
