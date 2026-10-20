//! USB-Serial-JTAG Communication Protocol & Frame Parser

pub mod usb_task;

use heapless::String;

pub const PROTO_PREFIX: &str = "@ISLAND:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActionState {
    #[default]
    Idle,
    Thinking,
    WaitConfirm,
    Success,
    Error,
}

impl core::str::FromStr for ActionState {
    type Err = core::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "THINKING" => ActionState::Thinking,
            "WAIT_CONFIRM" => ActionState::WaitConfirm,
            "SUCCESS" => ActionState::Success,
            "ERROR" => ActionState::Error,
            _ => ActionState::Idle,
        })
    }
}

impl ActionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionState::Idle => "IDLE",
            ActionState::Thinking => "THINKING",
            ActionState::WaitConfirm => "WAIT_CONFIRM",
            ActionState::Success => "SUCCESS",
            ActionState::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HostTelemetry {
    pub cpu_pct: u8,
    pub ram_pct: u8,
    pub rx_kbps: u32,
    pub tx_kbps: u32,
    pub cpu_temp: i8,
}

#[derive(Debug, Clone, Default)]
pub struct ActionNotification {
    pub state: ActionState,
    pub source: String<12>,
    pub title: String<32>,
    pub progress: u8, // 0..100
}

#[derive(Debug, Clone)]
pub struct TimeSync {
    pub epoch: u64,
    pub tz_offset_min: i16,
    pub media_info: String<32>,
}

#[derive(Debug, Clone)]
pub enum HostPacket {
    Telemetry(HostTelemetry),
    Action(ActionNotification),
    Sync(TimeSync),
    DumpLogs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    DoubleTap { context: &'static str },
    SingleTap { context: &'static str },
    FaceDown { entering: bool },
    Presence { is_near: bool, rssi: i8 },
    Heartbeat { v_rail: u16, uptime_sec: u32 },
    DumpLogsRequest,
}

impl core::fmt::Display for DeviceEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{PROTO_PREFIX}")?;
        match self {
            DeviceEvent::DoubleTap { context } => {
                writeln!(f, "EVT,type=DOUBLE_TAP,mode=ISLAND,context={context}")
            }
            DeviceEvent::SingleTap { context } => {
                writeln!(f, "EVT,type=SINGLE_TAP,mode=ISLAND,context={context}")
            }
            DeviceEvent::FaceDown { entering } => {
                let state = if *entering { "ENTER" } else { "EXIT" };
                writeln!(f, "EVT,type=FACE_DOWN,state={state}")
            }
            DeviceEvent::Presence { is_near, rssi } => {
                let state = if *is_near { "NEAR" } else { "AWAY" };
                writeln!(f, "EVT,type=PRESENCE,state={state},rssi={rssi}")
            }
            DeviceEvent::Heartbeat { v_rail, uptime_sec } => {
                writeln!(f, "EVT,type=HEARTBEAT,v_rail={v_rail},uptime={uptime_sec}")
            }
            DeviceEvent::DumpLogsRequest => {
                writeln!(f, "LOGS_DUMP_START")
            }
        }
    }
}

pub fn parse_line(line: &str) -> Option<HostPacket> {
    let trimmed = line.trim();
    if !trimmed.starts_with(PROTO_PREFIX) {
        return None;
    }

    let payload = &trimmed[PROTO_PREFIX.len()..];
    let mut parts = payload.splitn(2, ',');
    let cmd = parts.next()?;
    let args = parts.next().unwrap_or("");

    match cmd {
        "SYS" => parse_sys(args),
        "ACT" => parse_act(args),
        "SYNC" => parse_sync(args),
        "DUMP_LOGS" => Some(HostPacket::DumpLogs),
        _ => None,
    }
}

fn parse_sys(args: &str) -> Option<HostPacket> {
    let mut telem = HostTelemetry::default();
    for pair in args.split(',') {
        let mut kv = pair.splitn(2, '=');
        let Some(k) = kv.next().map(|s| s.trim()) else {
            continue;
        };
        let Some(v) = kv.next().map(|s| s.trim()) else {
            continue;
        };
        match k {
            "cpu" => telem.cpu_pct = v.parse().unwrap_or(0),
            "ram" => telem.ram_pct = v.parse().unwrap_or(0),
            "rx_kbps" => telem.rx_kbps = v.parse().unwrap_or(0),
            "tx_kbps" => telem.tx_kbps = v.parse().unwrap_or(0),
            "temp" => telem.cpu_temp = v.parse().unwrap_or(0),
            _ => {}
        }
    }
    Some(HostPacket::Telemetry(telem))
}

fn parse_act(args: &str) -> Option<HostPacket> {
    let mut act = ActionNotification::default();
    for pair in args.split(',') {
        let mut kv = pair.splitn(2, '=');
        let Some(k) = kv.next().map(|s| s.trim()) else {
            continue;
        };
        let Some(v) = kv.next().map(|s| s.trim()) else {
            if !act.title.is_empty() {
                let _ = act.title.push_str(", ");
                let _ = act.title.push_str(k);
            }
            continue;
        };
        match k {
            "state" => act.state = v.parse().unwrap_or_default(),
            "source" => {
                act.source.clear();
                let _ = act.source.push_str(v);
            }
            "title" => {
                act.title.clear();
                let _ = act.title.push_str(v);
            }
            "progress" => act.progress = v.parse().unwrap_or(0),
            _ => {}
        }
    }
    Some(HostPacket::Action(act))
}

fn parse_sync(args: &str) -> Option<HostPacket> {
    let mut epoch = 0u64;
    let mut tz = 480i16; // default UTC+8
    let mut media = String::<32>::new();

    for pair in args.split(',') {
        let mut kv = pair.splitn(2, '=');
        let Some(k) = kv.next().map(|s| s.trim()) else {
            continue;
        };
        let Some(v) = kv.next().map(|s| s.trim()) else {
            if !media.is_empty() {
                let _ = media.push_str(", ");
                let _ = media.push_str(k);
            }
            continue;
        };
        match k {
            "epoch" => epoch = v.parse().unwrap_or(0),
            "tz" => tz = v.parse().unwrap_or(480),
            "media" => {
                media.clear();
                let _ = media.push_str(v);
            }
            _ => {}
        }
    }

    Some(HostPacket::Sync(TimeSync {
        epoch,
        tz_offset_min: tz,
        media_info: media,
    }))
}
