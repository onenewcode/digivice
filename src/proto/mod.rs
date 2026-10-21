//! USB-Serial-JTAG Communication Protocol & Frame Parser

pub mod usb_task;

use crate::radio::TargetBinding;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseActionStateError {
    UnknownState,
}

impl core::fmt::Display for ParseActionStateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownState => write!(f, "Unknown action state string"),
        }
    }
}

impl core::error::Error for ParseActionStateError {}

impl core::str::FromStr for ActionState {
    type Err = ParseActionStateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "IDLE" => Ok(ActionState::Idle),
            "THINKING" => Ok(ActionState::Thinking),
            "WAIT_CONFIRM" => Ok(ActionState::WaitConfirm),
            "SUCCESS" => Ok(ActionState::Success),
            "ERROR" => Ok(ActionState::Error),
            _ => Err(ParseActionStateError::UnknownState),
        }
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ActionNotification {
    #[default]
    Idle,
    Thinking {
        source: String<12>,
        title: String<32>,
        progress: u8,
    },
    WaitConfirm {
        source: String<12>,
        title: String<32>,
    },
    Success {
        source: String<12>,
        title: String<32>,
    },
    Error {
        source: String<12>,
        title: String<32>,
    },
}

impl ActionNotification {
    pub const fn state(&self) -> ActionState {
        match self {
            Self::Idle => ActionState::Idle,
            Self::Thinking { .. } => ActionState::Thinking,
            Self::WaitConfirm { .. } => ActionState::WaitConfirm,
            Self::Success { .. } => ActionState::Success,
            Self::Error { .. } => ActionState::Error,
        }
    }

    pub fn source(&self) -> &str {
        match self {
            Self::Idle => "",
            Self::Thinking { source, .. }
            | Self::WaitConfirm { source, .. }
            | Self::Success { source, .. }
            | Self::Error { source, .. } => source.as_str(),
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Idle => "",
            Self::Thinking { title, .. }
            | Self::WaitConfirm { title, .. }
            | Self::Success { title, .. }
            | Self::Error { title, .. } => title.as_str(),
        }
    }

    pub const fn progress(&self) -> u8 {
        match self {
            Self::Thinking { progress, .. } => *progress,
            Self::Success { .. } => 100,
            _ => 0,
        }
    }
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
    BindBleTarget { binding: TargetBinding },
    ClearBleTarget,
    DumpLogs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapContext {
    NotifyClear,
    WaitConfirm,
    General,
}

impl TapContext {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NotifyClear => "NOTIFY_CLEAR",
            Self::WaitConfirm => "WAIT_CONFIRM",
            Self::General => "GENERAL",
        }
    }
}

impl core::fmt::Display for TapContext {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceDownTransition {
    Entering,
    Exiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceState {
    Near,
    Away,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    DoubleTap { context: TapContext },
    SingleTap { context: TapContext },
    FaceDown { transition: FaceDownTransition },
    Presence { state: PresenceState, rssi: i8 },
    Heartbeat { v_rail: u16, uptime_sec: u32 },
    DumpLogsRequest,
}

impl core::fmt::Display for DeviceEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{PROTO_PREFIX}")?;
        match self {
            DeviceEvent::DoubleTap { context } => {
                write!(f, "EVT,type=DOUBLE_TAP,mode=ISLAND,context={context}")
            }
            DeviceEvent::SingleTap { context } => {
                write!(f, "EVT,type=SINGLE_TAP,mode=ISLAND,context={context}")
            }
            DeviceEvent::FaceDown { transition } => {
                let state = match transition {
                    FaceDownTransition::Entering => "ENTER",
                    FaceDownTransition::Exiting => "EXIT",
                };
                write!(f, "EVT,type=FACE_DOWN,state={state}")
            }
            DeviceEvent::Presence { state, rssi } => {
                let state_str = match state {
                    PresenceState::Near => "NEAR",
                    PresenceState::Away => "AWAY",
                };
                write!(f, "EVT,type=PRESENCE,state={state_str},rssi={rssi}")
            }
            DeviceEvent::Heartbeat { v_rail, uptime_sec } => {
                write!(f, "EVT,type=HEARTBEAT,v_rail={v_rail},uptime={uptime_sec}")
            }
            DeviceEvent::DumpLogsRequest => {
                write!(f, "LOGS_DUMP_START")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtoParseError {
    InvalidPrefix,
    MissingCommand,
    UnknownCommand,
}

impl core::fmt::Display for ProtoParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidPrefix => write!(f, "Missing '@ISLAND:' protocol prefix"),
            Self::MissingCommand => write!(f, "Missing command identifier"),
            Self::UnknownCommand => write!(f, "Unrecognized command identifier"),
        }
    }
}

impl core::error::Error for ProtoParseError {}

pub fn parse_line(line: &str) -> Result<HostPacket, ProtoParseError> {
    let trimmed = line.trim();
    if !trimmed.starts_with(PROTO_PREFIX) {
        return Err(ProtoParseError::InvalidPrefix);
    }

    let payload = &trimmed[PROTO_PREFIX.len()..];
    let mut parts = payload.splitn(2, ',');
    let cmd = parts.next().ok_or(ProtoParseError::MissingCommand)?;
    let args = parts.next().unwrap_or("");

    match cmd {
        "SYS" => parse_sys(args).ok_or(ProtoParseError::UnknownCommand),
        "ACT" => parse_act(args).ok_or(ProtoParseError::UnknownCommand),
        "SYNC" => parse_sync(args).ok_or(ProtoParseError::UnknownCommand),
        "BLE_BIND" => parse_ble_bind(args).ok_or(ProtoParseError::UnknownCommand),
        "DUMP_LOGS" => Ok(HostPacket::DumpLogs),
        _ => Err(ProtoParseError::UnknownCommand),
    }
}

fn parse_ble_bind(args: &str) -> Option<HostPacket> {
    let mut hash = None;
    for pair in args.split(',') {
        let mut kv = pair.splitn(2, '=');
        let (Some(k), Some(v)) = (kv.next().map(str::trim), kv.next().map(str::trim)) else {
            continue;
        };
        if k == "hash" {
            hash = v.parse::<u16>().ok();
        }
    }
    match hash {
        Some(0) => Some(HostPacket::ClearBleTarget),
        Some(hash) => TargetBinding::new(hash).map(|binding| HostPacket::BindBleTarget { binding }),
        None => None,
    }
}

#[cfg(all(test, not(target_os = "none")))]
mod tests {
    use super::*;

    #[test]
    fn parse_ble_bind_accepts_valid_hash() {
        let packet = parse_line("@ISLAND:BLE_BIND,hash=43981").expect("valid BLE_BIND frame");
        assert!(matches!(
            packet,
            HostPacket::BindBleTarget { binding } if binding.hash() == 43981
        ));
    }

    #[test]
    fn parse_ble_bind_zero_clears_binding() {
        let packet = parse_line("@ISLAND:BLE_BIND,hash=0").expect("clear BLE_BIND frame");
        assert!(matches!(packet, HostPacket::ClearBleTarget));
    }

    #[test]
    fn parse_ble_bind_rejects_out_of_range_hash() {
        assert_eq!(
            parse_line("@ISLAND:BLE_BIND,hash=65536").map(|_| ()),
            Err(ProtoParseError::UnknownCommand)
        );
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

/// Safely append to a heapless String bounded by capacity and UTF-8 char boundaries
fn push_str_bounded<const N: usize>(target: &mut String<N>, s: &str) {
    let remaining = target.capacity().saturating_sub(target.len());
    if remaining > 0 {
        let max_len = s.floor_char_boundary(remaining.min(s.len()));
        let _ = target.push_str(&s[..max_len]);
    }
}

fn parse_act(args: &str) -> Option<HostPacket> {
    let mut state = ActionState::Idle;
    let mut source = String::<12>::new();
    let mut title = String::<32>::new();
    let mut progress = 0u8;

    for pair in args.split(',') {
        let mut kv = pair.splitn(2, '=');
        let Some(k) = kv.next().map(|s| s.trim()) else {
            continue;
        };
        let Some(v) = kv.next().map(|s| s.trim()) else {
            if !title.is_empty() {
                push_str_bounded(&mut title, ", ");
                push_str_bounded(&mut title, k);
            }
            continue;
        };
        match k {
            "state" => state = v.parse().unwrap_or_default(),
            "source" => {
                source.clear();
                push_str_bounded(&mut source, v);
            }
            "title" => {
                title.clear();
                push_str_bounded(&mut title, v);
            }
            "progress" => progress = v.parse().unwrap_or(0),
            _ => {}
        }
    }

    let act = match state {
        ActionState::Idle => ActionNotification::Idle,
        ActionState::Thinking => ActionNotification::Thinking {
            source,
            title,
            progress,
        },
        ActionState::WaitConfirm => ActionNotification::WaitConfirm { source, title },
        ActionState::Success => ActionNotification::Success { source, title },
        ActionState::Error => ActionNotification::Error { source, title },
    };

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
                push_str_bounded(&mut media, ", ");
                push_str_bounded(&mut media, k);
            }
            continue;
        };
        match k {
            "epoch" => epoch = v.parse().unwrap_or(0),
            "tz" => tz = v.parse().unwrap_or(480),
            "media" => {
                media.clear();
                push_str_bounded(&mut media, v);
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
