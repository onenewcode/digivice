#!/usr/bin/env python3
"""
Host Daemon for ESP32-C6 Desktop Island
Communicates with the ESP32-C6 over native USB-Serial-JTAG (VID:PID 303A:1001).

Features:
- Telemetry streaming (CPU, RAM, Network RX/TX, Temp) at 2~5Hz
- Clock synchronization (Epoch, Timezone)
- Action notifications for AI Agent / Compiler (Thinking, Wait Confirm, Success, Error)
- Uplink interaction handler (Double-tap approval, Single-tap dismiss, FaceDown DND, Presence Away lock)
"""

import argparse
import os
import platform
import subprocess
import sys
import time
from datetime import datetime, timezone

try:
    import serial
    import serial.tools.list_ports
except ImportError:
    serial = None

try:
    import psutil
except ImportError:
    psutil = None

ESPRESSIF_VID = 0x303A
JTAG_SERIAL_PID = 0x1001
PROTO_PREFIX = "@ISLAND:"


def find_esp32c6_port():
    """Auto-detect Espressif USB-Serial-JTAG port by VID:PID."""
    for port in serial.tools.list_ports.comports():
        if port.vid == ESPRESSIF_VID and port.pid == JTAG_SERIAL_PID:
            return port.device
    # Fallback search for common names
    for port in serial.tools.list_ports.comports():
        desc = (port.description or "").lower()
        if "usb-serial-jtag" in desc or "esp32-c6" in desc or "usbmodem" in desc:
            return port.device
    return None


def get_cpu_temp():
    """Try to get CPU temperature across platforms."""
    try:
        if hasattr(psutil, "sensors_temperatures"):
            temps = psutil.sensors_temperatures()
            if temps:
                for name in ("coretemp", "cpu_thermal", "k10temp", "soc_thermal"):
                    if name in temps and temps[name]:
                        return int(temps[name][0].current)
                first_entry = next(iter(temps.values()))
                if first_entry:
                    return int(first_entry[0].current)
    except Exception:
        pass
    return 48  # Reasonable default estimate if sensor unavailable


def handle_device_event(evt_str: str):
    """Handle incoming @ISLAND:EVT events from device."""
    print(f"\n[DEVICE EVENT] {evt_str}")
    parts = {}
    for item in evt_str.split(","):
        if "=" in item:
            k, v = item.split("=", 1)
            parts[k.strip()] = v.strip()

    evt_type = parts.get("type", "")

    if evt_type == "DOUBLE_TAP":
        ctx = parts.get("context", "")
        print(f"👉 >>> USER APPROVED ACTION (Double-Tap) [Context: {ctx}] <<< 👈")
        # Visual/audio bell
        sys.stdout.write("\a")
        sys.stdout.flush()

    elif evt_type == "SINGLE_TAP":
        ctx = parts.get("context", "")
        print(f"👉 User Single-Tapped device [Context: {ctx}]. Notification dismissed.")

    elif evt_type == "FACE_DOWN":
        state = parts.get("state", "")
        if state == "ENTER":
            print("🔕 Screen Face-Down on desk: Do Not Disturb activated.")
        else:
            print("🔔 Screen Face-Up: Resuming normal alerts.")

    elif evt_type == "PRESENCE":
        state = parts.get("state", "")
        rssi = parts.get("rssi", "")
        if state == "AWAY":
            print(f"🚶 User left desk (AWAY, RSSI {rssi} dBm). Triggering screen lock...")
            lock_desktop_screen()
        elif state == "NEAR":
            print(f"👋 User returned to desk (NEAR, RSSI {rssi} dBm). Welcome back!")

    elif evt_type == "HEARTBEAT":
        v_rail = parts.get("v_rail", "")
        uptime = parts.get("uptime", "")
        print(f"💓 Device Heartbeat: Rail Voltage={v_rail}mV, Uptime={uptime}s")


def lock_desktop_screen():
    """Trigger OS lock screen command on user departure."""
    system = platform.system().lower()
    try:
        if system == "darwin":
            subprocess.run(["pmset", "displaysleepnow"], check=False)
        elif system == "linux":
            subprocess.run(["loginctl", "lock-session"], check=False)
        elif system == "windows":
            import ctypes
            ctypes.windll.user32.LockWorkStation()
    except Exception as e:
        print(f"Could not lock screen: {e}")


def main():
    parser = argparse.ArgumentParser(description="Host Agent for ESP32-C6 Desktop Island")
    parser.add_argument("-p", "--port", type=str, help="Serial port (auto-detected if omitted)")
    parser.add_argument("-b", "--baud", type=int, default=115200, help="Serial baud rate")
    parser.add_argument("--notify", type=str, help="Inject action notification title and exit")
    parser.add_argument("--source", type=str, default="Host", help="Notification source name")
    parser.add_argument("--state", type=str, default="WAIT_CONFIRM",
                        choices=["IDLE", "THINKING", "WAIT_CONFIRM", "SUCCESS", "ERROR"],
                        help="Action notification state")
    parser.add_argument("--progress", type=int, default=100, help="Progress percentage (0-100)")
    parser.add_argument("--ble-hash", type=lambda x: int(x, 0), help="Bind target BLE hash (1-65535); 0 clears binding")
    parser.add_argument("--dump-logs", action="store_true", help="Request Flash blackbox dump")
    parser.add_argument("--duration", type=float, default=None, help="Run daemon for N seconds, then exit")

    args = parser.parse_args()

    if serial is None or psutil is None:
        missing = []
        if serial is None:
            missing.append("pyserial")
        if psutil is None:
            missing.append("psutil")
        print(f"Error: Missing required packages: {', '.join(missing)}. Install via: pip install {' '.join(missing)}")
        sys.exit(1)

    port_name = args.port or find_esp32c6_port()
    if not port_name:
        print("Error: Could not find ESP32-C6 USB-Serial-JTAG device. Check Type-C cable.")
        sys.exit(1)

    print(f"Connecting to ESP32-C6 on {port_name} ({args.baud} baud)...")
    try:
        ser = serial.Serial(port_name, args.baud, timeout=0.1)
    except Exception as e:
        print(f"Failed to open port {port_name}: {e}")
        sys.exit(1)

    # 1. One-shot command injection mode
    if args.notify:
        cmd = f"{PROTO_PREFIX}ACT,state={args.state},source={args.source},title={args.notify},progress={args.progress}\n"
        ser.write(cmd.encode("utf-8"))
        print(f"Sent: {cmd.strip()}")
        if args.duration is None:
            ser.close()
            sys.exit(0)

    if args.dump_logs:
        cmd = f"{PROTO_PREFIX}DUMP_LOGS\n"
        ser.write(cmd.encode("utf-8"))
        print("Requested Blackbox Flash dump from device...")
        deadline = time.time() + 1.0
        while time.time() < deadline:
            if ser.in_waiting:
                resp = ser.readline().decode("utf-8", errors="replace").strip()
                if resp:
                    print(f"Device: {resp}")
                    if "LOGS_DUMP" in resp:
                        break
            time.sleep(0.05)
        if args.duration is None:
            ser.close()
            sys.exit(0)

    if args.ble_hash is not None:
        if not 0 <= args.ble_hash <= 65535:
            parser.error("--ble-hash must be between 0 and 65535")
        cmd = f"{PROTO_PREFIX}BLE_BIND,hash={args.ble_hash}\n"
        ser.write(cmd.encode("utf-8"))
        print(f"Sent: {cmd.strip()}")
        if args.duration is None:
            ser.close()
            sys.exit(0)

    # 2. Continuous Daemon Mode: Sync Clock + Stream Telemetry
    print("Daemon running. Streaming telemetry (Ctrl+C to quit)...")

    # Initial Clock Sync
    now_utc = datetime.now(timezone.utc)
    epoch = int(now_utc.timestamp())
    # Local timezone offset in minutes
    local_tz = datetime.now().astimezone().tzinfo
    utcoffset = local_tz.utcoffset(None) if local_tz else None
    tz_offset = int((utcoffset.total_seconds() if utcoffset else 0) / 60)
    sync_frame = f"{PROTO_PREFIX}SYNC,epoch={epoch},tz={tz_offset},media=Active\n"
    ser.write(sync_frame.encode("utf-8"))

    prev_net = psutil.net_io_counters()
    prev_time = time.time()
    last_sync_time = time.time()
    started_at = time.time()

    try:
        while True:
            # 1. Read incoming serial lines from device
            while ser.in_waiting:
                line = ser.readline().decode("utf-8", errors="ignore").strip()
                if line:
                    if line.startswith(f"{PROTO_PREFIX}EVT,"):
                        handle_device_event(line[len(f"{PROTO_PREFIX}EVT,"):])
                    elif line.startswith("EVT,"):
                        handle_device_event(line[len("EVT,"):])
                    elif line.startswith(PROTO_PREFIX):
                        print(f"[DEVICE PROTO] {line}")
                    else:
                        # Normal debug log
                        print(f"[DEVICE LOG] {line}")

            # 2. Sample Telemetry every 500ms
            curr_time = time.time()
            dt = curr_time - prev_time
            if dt >= 0.5:
                curr_net = psutil.net_io_counters()
                rx_kbps = int(((curr_net.bytes_recv - prev_net.bytes_recv) * 8 / dt) / 1024)
                tx_kbps = int(((curr_net.bytes_sent - prev_net.bytes_sent) * 8 / dt) / 1024)
                prev_net = curr_net
                prev_time = curr_time

                cpu = int(psutil.cpu_percent())
                ram = int(psutil.virtual_memory().percent)
                temp = get_cpu_temp()

                sys_frame = f"{PROTO_PREFIX}SYS,cpu={cpu},ram={ram},rx_kbps={rx_kbps},tx_kbps={tx_kbps},temp={temp}\n"
                ser.write(sys_frame.encode("utf-8"))

            # 3. Periodic Clock Sync every 60s
            if curr_time - last_sync_time >= 60.0:
                last_sync_time = curr_time
                epoch = int(time.time())
                sync_frame = f"{PROTO_PREFIX}SYNC,epoch={epoch},tz={tz_offset},media=Active\n"
                ser.write(sync_frame.encode("utf-8"))

            if args.duration is not None and curr_time - started_at >= args.duration:
                print(f"Duration limit of {args.duration:.1f}s reached, exiting host agent.")
                break

            time.sleep(0.05)

    except KeyboardInterrupt:
        print("\nExiting host agent.")
    finally:
        ser.close()


if __name__ == "__main__":
    main()
