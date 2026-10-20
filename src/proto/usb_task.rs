//! USB-Serial-JTAG Async Link Task & Host Watchdog

pub use super::DeviceEvent;
use super::{HostPacket, parse_line};
use core::fmt::Write as _;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Instant};
use embedded_io_async::{Read, Write};
use esp_hal::Async;
use esp_hal::usb::usb_serial_jtag::{UsbSerialJtagRx, UsbSerialJtagTx};
use heapless::String;

pub type HostPacketChannel = Channel<CriticalSectionRawMutex, HostPacket, 8>;
pub type DeviceEventChannel = Channel<CriticalSectionRawMutex, DeviceEvent, 8>;

pub static HOST_PACKET_CHANNEL: HostPacketChannel = Channel::new();
pub static DEVICE_EVENT_CHANNEL: DeviceEventChannel = Channel::new();

pub struct UsbLink<'d> {
    rx: UsbSerialJtagRx<'d, Async>,
    tx: UsbSerialJtagTx<'d, Async>,
    line_buf: [u8; 256],
    line_len: usize,
    last_rx_time: Instant,
    host_connected: bool,
}

impl<'d> UsbLink<'d> {
    pub fn new(rx: UsbSerialJtagRx<'d, Async>, tx: UsbSerialJtagTx<'d, Async>) -> Self {
        Self {
            rx,
            tx,
            line_buf: [0u8; 256],
            line_len: 0,
            last_rx_time: Instant::now(),
            host_connected: false,
        }
    }

    /// Read bytes and push parsed HostPacket to HOST_PACKET_CHANNEL
    pub async fn process_rx_chunk(&mut self) {
        let mut temp_buf = [0u8; 64];
        let n: usize = self.rx.read(&mut temp_buf).await.unwrap_or_default();
        if n > 0 {
            self.last_rx_time = Instant::now();
            self.host_connected = true;

            for &byte in &temp_buf[..n] {
                if (byte == b'\n' || byte == b'\r') && self.line_len > 0 {
                    if let Some(packet) = core::str::from_utf8(&self.line_buf[..self.line_len])
                        .ok()
                        .and_then(parse_line)
                    {
                        HOST_PACKET_CHANNEL.send(packet).await;
                    }
                    self.line_len = 0;
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

    /// Check if host connection has timed out (> 10s)
    pub fn check_watchdog(&mut self, now: Instant) -> bool {
        if self.host_connected && now.duration_since(self.last_rx_time) > Duration::from_secs(10) {
            self.host_connected = false;
        }
        self.host_connected
    }

    /// Send formatted string over USB
    pub async fn send_str(&mut self, s: &str) {
        let _ = self.tx.write_all(s.as_bytes()).await;
    }

    /// Send DeviceEvent over USB
    pub async fn send_event(&mut self, evt: &DeviceEvent) {
        let mut msg = String::<96>::new();
        if write!(msg, "{evt}").is_ok() {
            self.send_str(&msg).await;
        }
    }
}
