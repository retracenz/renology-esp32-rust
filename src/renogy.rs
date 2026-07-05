//! Renogy BT protocol: Modbus RTU frames carried over BLE GATT
//! (write requests to 0xFFD1, receive responses as notifications on 0xFFF1).
//!
//! Protocol as implemented by cyrils/renogy-bt and IAmTheMitchell/renogy-ble
//! for the RBC/DCC series DC-DC chargers.

/// Renogy BT devices advertise a name starting with this prefix.
pub const NAME_PREFIX: &str = "BT-TH";

/// GATT service/characteristic holding the Modbus write characteristic.
pub const WRITE_SERVICE: u16 = 0xFFD0;
pub const WRITE_CHAR: u16 = 0xFFD1;
/// GATT service/characteristic delivering Modbus responses as notifications.
pub const NOTIFY_SERVICE: u16 = 0xFFF0;
pub const NOTIFY_CHAR: u16 = 0xFFF1;

/// Broadcast device id; works for a stand-alone (non-hub) charger.
pub const DEVICE_ID: u8 = 0xFF;

/// Start of the DC-DC charger's dynamic data block.
pub const DYNAMIC_REG: u16 = 0x0100;
pub const DYNAMIC_WORDS: u16 = 30;

/// Live values decoded from the dynamic block at 0x0100.
#[derive(Clone, Copy, Debug, Default)]
pub struct ChargerData {
    /// House (auxiliary/service) battery state of charge, percent.
    pub soc: u16,
    /// House battery voltage, tenths of a volt.
    pub house_volts_x10: u16,
    /// Combined charge current (alternator + solar), hundredths of an amp.
    pub charge_amps_x100: u16,
    /// Alternator/starter battery voltage, tenths of a volt.
    pub start_volts_x10: u16,
    /// Alternator current, hundredths of an amp.
    pub alt_amps_x100: u16,
    /// Alternator power, watts.
    pub alt_watts: u16,
    /// Solar input voltage, tenths of a volt.
    pub solar_volts_x10: u16,
    /// Solar input current, hundredths of an amp.
    pub solar_amps_x100: u16,
    /// Solar power, watts.
    pub solar_watts: u16,
}

impl ChargerData {
    /// Decode from the register data of a read of DYNAMIC_REG.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 20 {
            return None;
        }
        Some(Self {
            soc: word(data, 0),
            house_volts_x10: word(data, 1),
            charge_amps_x100: word(data, 2),
            // word 3 holds controller/battery temperatures
            start_volts_x10: word(data, 4),
            alt_amps_x100: word(data, 5),
            alt_watts: word(data, 6),
            solar_volts_x10: word(data, 7),
            solar_amps_x100: word(data, 8),
            solar_watts: word(data, 9),
        })
    }
}

/// Standard Modbus CRC16 (poly 0xA001, init 0xFFFF).
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= byte as u16;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

/// Build a Modbus "read holding registers" (function 0x03) request frame.
pub fn read_request(device_id: u8, start_reg: u16, words: u16) -> [u8; 8] {
    let mut frame = [
        device_id,
        0x03,
        (start_reg >> 8) as u8,
        start_reg as u8,
        (words >> 8) as u8,
        words as u8,
        0,
        0,
    ];
    let crc = crc16(&frame[..6]);
    frame[6] = crc as u8; // CRC low byte first
    frame[7] = (crc >> 8) as u8;
    frame
}

/// Accumulates notification fragments until a complete, CRC-valid Modbus
/// response frame is present. Responses longer than one BLE notification
/// arrive as multiple fragments.
pub struct ResponseBuffer {
    buf: [u8; 256],
    len: usize,
}

impl ResponseBuffer {
    pub fn new() -> Self {
        Self { buf: [0; 256], len: 0 }
    }

    pub fn reset(&mut self) {
        self.len = 0;
    }

    /// Feed one notification payload. Returns the register data words once a
    /// complete function-0x03 response has been assembled and CRC-checked.
    pub fn feed(&mut self, fragment: &[u8]) -> Option<&[u8]> {
        let space = self.buf.len() - self.len;
        let take = fragment.len().min(space);
        self.buf[self.len..self.len + take].copy_from_slice(&fragment[..take]);
        self.len += take;

        if self.len < 3 {
            return None;
        }
        if self.buf[1] != 0x03 {
            // Error response (function | 0x80) or garbage: drop it.
            self.len = 0;
            return None;
        }
        let byte_count = self.buf[2] as usize;
        let frame_len = 3 + byte_count + 2;
        if self.len < frame_len {
            return None;
        }
        let frame = &self.buf[..frame_len];
        let crc = crc16(&frame[..frame_len - 2]);
        let sent = u16::from_le_bytes([frame[frame_len - 2], frame[frame_len - 1]]);
        self.len = 0;
        if crc == sent {
            Some(&self.buf[3..3 + byte_count])
        } else {
            None
        }
    }
}

impl Default for ResponseBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a big-endian 16-bit register word out of response data.
pub fn word(data: &[u8], index: usize) -> u16 {
    u16::from_be_bytes([data[2 * index], data[2 * index + 1]])
}
