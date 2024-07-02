use std::{
    fmt::Display,
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    slice::{from_raw_parts, from_raw_parts_mut},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Utc};

use crate::tryfrom_enum;

static NTP_VERSION: u8 = 4;
static NTP_PACKET_SIZE: usize = 48;

// Seconds between ntp time reference and Unix time reference
static NTP_TO_UNIX_DURATION: Duration = Duration::new(2208988800, 0);

pub type Version = u8;

#[derive(Debug)]
pub struct NtpError {
    msg: String,
}

tryfrom_enum! {
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode(repr(u8)) {
    Reserved = 0x0,
    Client = 0x3,
    Server = 0x4,
}
}

tryfrom_enum! {
#[derive(Debug, Clone, Copy)]
enum Leap(repr(u8)) {
    NoWarning = 0x0,
    LastHas61 = 0x1,
    LastHas59 = 0x2,
    Unknown = 0x3,
}
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TimeStamp {
    time: u64,
}

#[derive(Debug)]
pub struct NtpNet {
    socket: UdpSocket,
    server_addr: SocketAddr,
}

#[derive(Debug)]
pub struct NtpSync {
    net: NtpNet,
}

#[repr(C)]
#[derive(Default, Debug)]
pub struct Packet {
    flags: u8,          /* leap, version and mode */
    stratum: u8,        /* stratum */
    poll: i8,           /* poll interval */
    precision: i8,      /* precision */
    rootdelay: u32,     /* root delay */
    rootdisp: u32,      /* root dispersion */
    refid: u32,         /* reference ID */
    reftime: TimeStamp, /* reference time */
    org: TimeStamp,     /* origin timestamp */
    rec: TimeStamp,     /* receive timestamp */
    xmt: TimeStamp,     /* transmit timestamp */
}

pub struct SyncResult {
    estimated_time: SystemTime,
    offset: f64,
    delay: f64,
    leap: Leap,
}

impl Display for NtpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.msg.as_str())
    }
}

impl NtpError {
    fn from_string(msg: String) -> Self {
        Self { msg }
    }

    fn from_slice(msg: &str) -> Self {
        Self::from_string(String::from(msg))
    }
}

// Set time to 0: Special meaning for NTP
impl Default for TimeStamp {
    fn default() -> Self {
        Self { time: 0 }
    }
}

impl TimeStamp {
    fn now() -> Self {
        let now = SystemTime::now();
        let unix_timestamp = now
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get local time");
        let ntp_timestamp = unix_timestamp + NTP_TO_UNIX_DURATION;

        let sec = ntp_timestamp.as_secs();
        let nano = ntp_timestamp.subsec_nanos();
        Self {
            time: u64::to_be(((sec as u64) << 32) | (nano as u64)),
        }
    }

    fn raw(&self) -> u64 {
        self.time
    }

    fn decode(&self) -> Option<SystemTime> {
        let cpu_repr = u64::from_be(self.time);
        let sec = cpu_repr >> 32;
        let nano = (cpu_repr & 0xffffffff) as u32;
        if sec < NTP_TO_UNIX_DURATION.as_secs() {
            return None;
        }
        let duration = Duration::new(sec, nano) - NTP_TO_UNIX_DURATION;
        Some(SystemTime::UNIX_EPOCH + duration)
    }
}

impl Display for Leap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Leap::NoWarning => "No leap",
            Leap::LastHas61 => "Last minute has 61 seconds",
            Leap::LastHas59 => "Last minute has 59 seconds",
            Leap::Unknown => "Unknown (clock unsynchronized)",
        };
        f.write_str(msg)
    }
}

impl Packet {
    fn new_client_packet() -> Self {
        let timestamp = TimeStamp::now();
        Self {
            flags: Self::flags(Leap::Unknown, NTP_VERSION, Mode::Client),
            stratum: 0,
            poll: 0,
            precision: 0,
            rootdelay: 0,
            rootdisp: 0,
            refid: 0,
            reftime: TimeStamp::default(),
            org: TimeStamp::default(),
            rec: TimeStamp::default(),
            xmt: timestamp,
        }
    }

    fn flags(leap: Leap, version: Version, mode: Mode) -> u8 {
        ((leap as u8) << 6) | (version << 3) | (mode as u8)
    }

    fn leap(&self) -> Option<Leap> {
        Leap::try_from(self.flags >> 6).ok()
    }

    fn mode(&self) -> Option<Mode> {
        Mode::try_from(self.flags & 0x7).ok()
    }

    fn duration_since_unix_epoch(t: TimeStamp) -> Option<Duration> {
        t.decode()?.duration_since(UNIX_EPOCH).ok()
    }

    // returns the durations since unix epoch
    fn timestamps(&self) -> Option<(Duration, Duration, Duration)> {
        let t1 = Self::duration_since_unix_epoch(self.org)?;
        let t2 = Self::duration_since_unix_epoch(self.rec)?;
        let t3 = Self::duration_since_unix_epoch(self.xmt)?;
        Some((t1, t2, t3))
    }
}

impl NtpNet {
    pub fn new(server: &str) -> Result<Self, NtpError> {
        let mut addr_iter = match server.to_socket_addrs() {
            Ok(a) => a,
            Err(e) => return Err(NtpError::from_string(e.to_string())),
        };
        let Some(server_addr) = addr_iter.next() else {
            return Err(NtpError::from_slice("Invalid server address"));
        };

        let addr = if server_addr.is_ipv4() {
            "0.0.0.0"
        } else {
            "::"
        };
        
        let socket = match UdpSocket::bind((addr, 0)) {
            Ok(s) => s,
            Err(e) => return Err(NtpError::from_string(e.to_string())),
        };

        Ok(Self {
            socket,
            server_addr,
        })
    }

    pub fn send_packet(&self, packet: &Packet) -> Result<(), NtpError> {
        let buf = unsafe {
            from_raw_parts(
                (packet as *const Packet) as *const u8,
                NTP_PACKET_SIZE,
            )
        };
        match self.socket.send_to(buf, &self.server_addr) {
            Ok(_) => Ok(()),
            Err(e) => Err(NtpError::from_string(e.to_string())),
        }
    }

    pub fn receive_packet(&self, packet: &mut Packet) -> Result<(), NtpError> {
        // no copy
        let buf = unsafe {
            from_raw_parts_mut(
                (packet as *mut Packet) as *mut u8,
                NTP_PACKET_SIZE,
            )
        };
        let (size, sender) = match self.socket.recv_from(buf) {
            Ok(res) => res,
            Err(e) => return Err(NtpError::from_string(e.to_string())),
        };

        if sender.ip() != self.server_addr.ip()
            || sender.port() != self.server_addr.port()
            || size != NTP_PACKET_SIZE
        {
            return Err(NtpError::from_slice("Unexpected reply"));
        }

        Ok(())
    }
}

impl SyncResult {
    fn new(offset: f64, delay: f64, leap: Leap) -> Self {
        let estimated_time =
            SystemTime::now() + Duration::from_secs_f64(offset);
        Self {
            estimated_time,
            offset,
            delay,
            leap,
        }
    }
}

impl Display for SyncResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let datetime = DateTime::<Utc>::from(self.estimated_time);
        f.write_fmt(format_args!(
            "{:?} / offset: {}, delay: {}, leap: {}",
            datetime, self.offset, self.delay, self.leap
        ))
    }
}

impl NtpSync {
    pub fn new(server: &str) -> Result<Self, NtpError> {
        let net = NtpNet::new(server)?;
        Ok(Self { net })
    }

    fn should_discard_received_packet(&self, packet: &Packet) -> bool {
        let Some(mode) = packet.mode() else {
            return true;
        };

        packet.stratum == 0
            || mode != Mode::Server
            || packet.org.raw() == 0
            || packet.rec.raw() == 0
            || packet.xmt.raw() == 0
    }

    pub fn sync(&mut self) -> Result<SyncResult, NtpError> {
        let out_packet = Packet::new_client_packet();
        self.net.send_packet(&out_packet)?;

        let mut reply = Packet::default();

        // receive and get time
        self.net.receive_packet(&mut reply)?;
        let t4 = SystemTime::now();

        if self.should_discard_received_packet(&reply) {
            return Err(NtpError::from_slice(
                "Incorrect server reply content, discarded",
            ));
        }

        self.compute_time(&reply, t4)
    }

    fn compute_time(
        &self, packet: &Packet, t4: SystemTime,
    ) -> Result<SyncResult, NtpError> {
        let (t1, t2, t3) = match packet.timestamps() {
            Some(r) => r,
            None => {
                return Err(NtpError::from_slice(
                    "Failed to decode timestamps from reply",
                ))
            }
        };
        let Some(leap) = packet.leap() else {
            return Err(NtpError::from_slice(
                "Failed to decode leap field in the reply",
            ));
        };

        let t4 = t4
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get local time");
        let ft1 = t1.as_secs_f64();
        let ft2 = t2.as_secs_f64();
        let ft3 = t3.as_secs_f64();
        let ft4 = t4.as_secs_f64();
        let offset = ((ft2 - ft1) + (ft3 - ft4)) / 2.0;
        let delay = (ft4 - ft1) - (ft3 - ft2);

        Ok(SyncResult::new(offset, delay, leap))
    }
}
