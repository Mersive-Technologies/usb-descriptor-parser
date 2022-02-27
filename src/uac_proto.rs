#![allow(dead_code)] // TODO: tests around all code

use std::io::{Write, Read};
use crate::usb_proto::UsbDescriptorTypes;
use anyhow::Error;
use structure::byteorder::{WriteBytesExt, ReadBytesExt};

#[derive(FromPrimitive, Debug)]
#[repr(u16)]
pub enum FeatureUnitControlSelectors {
    UacFuMute = 0x01,
    UacFuVolume = 0x02,
    UacFuBass = 0x03,
    UacFuMid = 0x04,
    UacFuTreble = 0x05,
    UacFuGraphicEqualizer = 0x06,
    UacFuAutomaticGain = 0x07,
    UacFuDelay = 0x08,
    UacFuBassBoost = 0x09,
    UacFuLoudness = 0x0a,
}

#[derive(FromPrimitive, Debug)]
#[repr(u8)]
pub enum UacRequestCodes {
    Undefined = 0x00,
    SetCur = 0x01,
    GetCur = 0x81,
    GetMin = 0x82,
    GetMax = 0x83,
    GetResolution = 0x84,
    GetLen = 0x85,
    GetInfo = 0x86,
    GetDef = 0x87,
}

// UAC
// https://www.usb.org/sites/default/files/audio10.pdf
// https://github.com/torvalds/linux/blob/master/include/uapi/linux/usb/audio.h

#[derive(FromPrimitive)]
#[repr(u8)]
pub enum UacInterfaceSubclass {
    AudioControl = 0x01,
    AudioStreaming = 0x02,
    MidiStreaming = 0x03,
}

#[derive(FromPrimitive)]
#[repr(u8)]
pub enum UacDescriptorSubtypes {
    Header = 0x01,
    InputTerminal = 0x02,
    OutputTerminal = 0x03,
    MixerUnit = 0x04,
    SelectorUnit = 0x05,
    FeatureUnit = 0x06,
    ProcessingUnit = 0x07,
    ExtensionUnit = 0x08,
}

#[derive(FromPrimitive)]
#[repr(u8)]
pub enum UacInterfaceSubtypes {
    General = 0x01,
    FormatType = 0x02,
    FormatSpecific = 0x03,
}

#[derive(FromPrimitive)]
#[repr(u8)]
pub enum UacFormatTypeI {
    Undefined = 0x0,
    Pcm = 0x1,
    Pcm8 = 0x2,
    IeeeFloat = 0x3,
    Alaw = 0x4,
    Mulaw = 0x5,
}

#[derive(Debug, Clone)]
pub struct Uac1AcHeaderDescriptor {
    pub bcd_adc: u16,
    pub w_total_length: u16,
    pub b_in_collection: u8,
    pub ba_interface_nr: Vec<u8>,
}

impl Uac1AcHeaderDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBHHB");
        let sz = format.size() as u8 + self.ba_interface_nr.len() as u8;
        format.pack_into(
            &mut buffer, sz, UsbDescriptorTypes::CsInterface as u8, UacDescriptorSubtypes::Header as u8,
            self.bcd_adc, self.w_total_length, self.b_in_collection
        ).unwrap();
        buffer.write_all(&self.ba_interface_nr).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> Uac1AcHeaderDescriptor {
        let format = structure!("<HHB");
        let (bcd_adc, w_total_length, b_in_collection) = format.unpack_from(&mut buffer).unwrap();
        let sz = b_in_collection;
        let ba_interface_nr = (0..sz).map(|_| buffer.read_u8().unwrap()).collect();
        let msg = Uac1AcHeaderDescriptor { bcd_adc, w_total_length, b_in_collection, ba_interface_nr };
        return msg;
    }
}

#[derive(Debug, Clone)]
pub struct Uac1OutputTerminalDescriptor {
    pub b_terminal_id: u8,
    pub w_terminal_type: u16,
    pub b_assoc_terminal: u8,
    pub b_source_id: u8,
    pub i_terminal: u8,
}

impl Uac1OutputTerminalDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBHBBB");
        let sz = format.size() as u8;
        format.pack_into(
            &mut buffer, sz, UsbDescriptorTypes::CsInterface as u8, UacDescriptorSubtypes::OutputTerminal as u8,
            self.b_terminal_id, self.w_terminal_type, self.b_assoc_terminal, self.b_source_id, self.i_terminal
        ).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> Uac1OutputTerminalDescriptor {
        let format = structure!("<BHBBB");
        let (b_terminal_id, w_terminal_type, b_assoc_terminal, b_source_id, i_terminal) = format.unpack_from(&mut buffer).unwrap();
        let msg = Uac1OutputTerminalDescriptor { b_terminal_id, w_terminal_type, b_assoc_terminal, b_source_id, i_terminal };
        return msg;
    }
}

#[derive(Debug, Clone)]
pub struct UacInputTerminalDescriptor {
    pub b_terminal_id: u8,
    pub w_terminal_type: u16,
    pub b_assoc_terminal: u8,
    pub b_nr_channels: u8,
    pub w_channel_config: u16,
    pub i_channel_names: u8,
    pub i_terminal: u8,
}

impl UacInputTerminalDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBHBBHBB");
        let sz = format.size() as u8;
        format.pack_into(
            &mut buffer, sz, UsbDescriptorTypes::CsInterface as u8, UacDescriptorSubtypes::InputTerminal as u8,
            self.b_terminal_id, self.w_terminal_type, self.b_assoc_terminal, self.b_nr_channels, self.w_channel_config, self.i_channel_names, self.i_terminal
        ).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> UacInputTerminalDescriptor {
        let format = structure!("<BHBBHBB");
        let (b_terminal_id, w_terminal_type, b_assoc_terminal, b_nr_channels, w_channel_config, i_channel_names, i_terminal) = format.unpack_from(&mut buffer).unwrap();
        let msg = UacInputTerminalDescriptor { b_terminal_id, w_terminal_type, b_assoc_terminal, b_nr_channels, w_channel_config, i_channel_names, i_terminal };
        return msg;
    }
}

#[derive(Debug, Clone)]
pub struct UacFeatureUnitDescriptor {
    pub b_unit_id: u8,
    pub b_source_id: u8,
    pub b_control_size: u8,
    pub bma_controls: Vec<u8>,
    // TODO: last bma_controls byte can actually be an iTerminal - use number of channels, not b_length
}

impl UacFeatureUnitDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) -> Result<(), Error> {
        let format = structure!("<BBBBBB");
        let sz = format.size() as u8 + self.bma_controls.len() as u8;
        format.pack_into(
            &mut buffer, sz, UsbDescriptorTypes::CsInterface as u8, UacDescriptorSubtypes::FeatureUnit as u8,
            self.b_unit_id, self.b_source_id, self.b_control_size
        )?;
        let _ = buffer.write_all(&self.bma_controls);
        Ok(())
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> Result<UacFeatureUnitDescriptor, Error> {
        let format = structure!("<BBB");
        let sz = buffer.len() - format.size();
        let (b_unit_id, b_source_id, b_control_size) = format.unpack_from(&mut buffer)?;
        let mut bma_controls = vec![0u8; sz];
        let _ = buffer.read_exact(&mut bma_controls);
        let msg = UacFeatureUnitDescriptor { b_unit_id, b_source_id, b_control_size, bma_controls };
        Ok(msg)
    }
}

#[derive(Debug, Clone)]
pub struct Uac1AsHeaderDescriptor {
    pub b_terminal_link: u8,
    pub b_delay: u8,
    pub w_format_tag: u16,
}

impl Uac1AsHeaderDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBBH");
        format.pack_into(&mut buffer, format.size() as u8, UsbDescriptorTypes::CsInterface as u8, UacInterfaceSubtypes::General as u8, self.b_terminal_link, self.b_delay, self.w_format_tag).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> Uac1AsHeaderDescriptor {
        let format = structure!("<BBH");
        let (b_terminal_link, b_delay, w_format_tag) = format.unpack_from(&mut buffer).unwrap();
        let msg = Uac1AsHeaderDescriptor { b_terminal_link, b_delay, w_format_tag };
        return msg;
    }
}

#[derive(Debug, Clone)]
pub struct UacIsoEndpointDescriptor {
    pub b_descriptor_subtype: u8,
    pub bm_attributes: u8,
    pub b_lock_delay_units: u8,
    pub w_lock_delay: u16,
}

impl UacIsoEndpointDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBBH");
        format.pack_into(&mut buffer, format.size() as u8, UsbDescriptorTypes::CsEndpoint as u8, self.b_descriptor_subtype, self.bm_attributes, self.b_lock_delay_units, self.w_lock_delay).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> Result<UacIsoEndpointDescriptor, Error> {
        let format = structure!("<BBBH");
        let (b_descriptor_subtype, bm_attributes, b_lock_delay_units, w_lock_delay) = format.unpack_from(&mut buffer)?;
        let msg = UacIsoEndpointDescriptor { b_descriptor_subtype, bm_attributes, b_lock_delay_units, w_lock_delay };
        Ok(msg)
    }
}

#[derive(Debug, Clone)]
pub struct UacFormatTypeIContinuousDescriptor {
    pub b_nr_channels: u8,
    pub b_subframe_size: u8,
    pub b_bit_resolution: u8,
    pub b_sam_freq_type: u8,
    pub t_sam_freq: Vec<u32>,
}

impl UacFormatTypeIContinuousDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) -> Result<(), Error> {
        let format = structure!("<BBBBBBBB");
        let sz = format.size() as u8 + self.t_sam_freq.len() as u8 * 3u8;
        format.pack_into(&mut buffer, sz, UsbDescriptorTypes::CsInterface as u8, UacInterfaceSubtypes::FormatType as u8, UacFormatTypeI::Pcm as u8, self.b_nr_channels, self.b_subframe_size,
                         self.b_bit_resolution, self.b_sam_freq_type,
        )?;
        for freq in &self.t_sam_freq {
            buffer.write_u8((freq >> 0) as u8)?;
            buffer.write_u8((freq >> 8) as u8)?;
            buffer.write_u8((freq >> 16) as u8)?;
        }
        Ok(())
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> Result<UacFormatTypeIContinuousDescriptor, Error> {
        let format = structure!("<BBBB");
        let (b_nr_channels, b_subframe_size, b_bit_resolution, b_sam_freq_type) = format.unpack_from(&mut buffer)?;
        let sz = b_sam_freq_type;
        let t_sam_freq = (0..sz).map(|_| {
            let mut bytes = [0u8; 3];
            buffer.read_exact(&mut bytes).unwrap();
            (bytes[2] as u32) << 16 | (bytes[1] as u32) << 8 | (bytes[0] as u32)
        }).collect();
        let msg = UacFormatTypeIContinuousDescriptor { b_nr_channels, b_subframe_size, b_bit_resolution, b_sam_freq_type, t_sam_freq };
        Ok(msg)
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorUacInterfaceUnknown {
    pub iface_subclass: u8,
    pub bytes: Vec<u8>,
}

impl DescriptorUacInterfaceUnknown {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBB");
        format.pack_into(
            &mut buffer, self.bytes.len() as u8 + 3u8, UsbDescriptorTypes::CsInterface as u8, self.iface_subclass
        ).unwrap();
        buffer.write_all(&self.bytes).unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorUacFormatTypeUnknown {
    pub format_type: u8,
    pub bytes: Vec<u8>,
}

impl DescriptorUacFormatTypeUnknown {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBB");
        format.pack_into(
            &mut buffer, self.bytes.len() as u8 + format.size() as u8, UsbDescriptorTypes::CsInterface as u8,
            UacInterfaceSubtypes::FormatType as u8, self.format_type
        ).unwrap();
        buffer.write_all(&self.bytes).unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct UacVolume {
    min: i16,
    max: i16,
    pub cur: i16,
}

impl UacVolume {
    /// UAC volume settings are provided as 16-bit values which correspond to decibels.
    /// Creates a new UacVolume object from these values.
    pub fn new(min: i16, max: i16, cur: i16) -> Result<UacVolume, Error> {
        if min >= max || min < UacVolume::DB_MIN {
            Err(anyhow!("UacVolume(min: {}; max: {}, cur: {}): Invalid min", min, max, cur))
        } else if max <= min || max > UacVolume::DB_MAX {
            Err(anyhow!("UacVolume(min: {}; max: {}, cur: {}): Invalid max", min, max, cur))
        } else if cur > max || (cur < min && cur != UacVolume::DB_SILENCE) {
            Err(anyhow!("UacVolume(min: {}; max: {}, cur: {}): Invalid cur", min, max, cur))
        } else {
            Ok(UacVolume{ min, max, cur })
        }
    }

    /// Converts the current volume to a normalized in range of 0.0 - 1.0.
    pub fn cur_normalized(&self) -> f32 {
        let range = self.max as i32 - self.min as i32;
        (self.cur as i32 - self.min as i32) as f32 / range as f32
    }

    /// Converts the normalized volume to its decibel value.
    pub fn normalized_to_db(&self, normalized: f32) -> f32 {
        let range = self.max as i32 - self.min as i32;
        let denormalized = normalized * range as f32 + self.min as f32;
        UacVolume::to_db(denormalized as i16)
    }

    /// Returns the decibel range.
    pub fn db_range(&self) -> f32 {
        UacVolume::to_db(self.max) - UacVolume::to_db(self.min)
    }

    /// Returns true if the current volume is set to the special SILENCE value.
    pub fn is_silent(&self) -> bool {
        self.cur == UacVolume::DB_SILENCE
    }

    /// From section 5.2.2.4.3.2 of USB Device Class Definition for Audio Devices v1.0 (Audio10.pdf)
    const DB_MIN: i16 = i16::MIN + 1;        /// Minimum allowed decibel level
    const DB_MAX: i16 = i16::MAX;            /// Maximum allowed decibel level
    const DB_SILENCE: i16 = i16::MIN;        /// Special value corresponding to silence
    const DB_RES_MAX: f32 = 1f32 / 256f32;   /// Maximum decibel step resolution.
    const _DB_RES_MIN: f32 = i16::MAX as f32; /// Minimum decibel step resolution.

    /// Converts a given volume to its decibel value.
    pub fn to_db(volume: i16) -> f32 {
        volume as f32 * UacVolume::DB_RES_MAX
    }
}

#[cfg(test)]
mod test {
    use crate::uac_proto::UacVolume;
    use float_eq::float_eq;

    #[test]
    fn uac_vol_min_test() {
        let actual = UacVolume::to_db(UacVolume::DB_MIN);
        let expected = -127.9961f32;
        assert!(float_eq!(actual, expected, abs <= 0.000_1));
    }

    #[test]
    fn uac_vol_max_test() {
        let actual = UacVolume::to_db(UacVolume::DB_MAX);
        let expected = 127.9961f32;
        assert!(float_eq!(actual, expected, abs <= 0.000_1));
    }

    #[test]
    fn uac_vol_silence_test() {
        let actual = UacVolume::to_db(UacVolume::DB_SILENCE);
        let expected = -128.0;
        assert!(float_eq!(actual, expected, abs <= 0.000_1));
    }

    #[test]
    fn uac_vol_max_db_range_test() {
        let uac_vol = UacVolume::new(UacVolume::DB_MIN, UacVolume::DB_MAX, 0).unwrap();
        let actual = uac_vol.db_range();
        let expected = 256.0;
        assert!(float_eq!(actual, expected, abs <= 0.01));
    }

    #[test]
    fn uac_vol_cur_to_normalized_test() {
        // Max
        let uac_vol = UacVolume::new(UacVolume::DB_MIN, UacVolume::DB_MAX, UacVolume::DB_MAX).unwrap();
        let actual = uac_vol.cur_normalized();
        let expected = 1.0;
        assert!(float_eq!(actual, expected, abs <= 0.000_1));

        // Min
        let uac_vol = UacVolume::new(UacVolume::DB_MIN, UacVolume::DB_MAX, UacVolume::DB_MIN).unwrap();
        let actual = uac_vol.cur_normalized();
        let expected = 0.0;
        assert!(float_eq!(actual, expected, abs <= 0.000_1));

        // Mid
        let uac_vol = UacVolume::new(UacVolume::DB_MIN, UacVolume::DB_MAX, 0).unwrap();
        let actual = uac_vol.cur_normalized();
        let expected = 0.5;
        assert!(float_eq!(actual, expected, abs <= 0.000_1));
    }

    #[test]
    fn uac_vol_normalized_to_db_test() {
        // Max
        let uac_vol = UacVolume::new(UacVolume::DB_MIN, UacVolume::DB_MAX, 0).unwrap();
        let actual = uac_vol.normalized_to_db(1.0);
        let expected = UacVolume::to_db(UacVolume::DB_MAX);
        assert!(float_eq!(actual, expected, abs <= 0.000_1));

        // Min
        let uac_vol = UacVolume::new(UacVolume::DB_MIN, UacVolume::DB_MAX, 0).unwrap();
        let actual = uac_vol.normalized_to_db(0.0);
        let expected = UacVolume::to_db(UacVolume::DB_MIN);
        assert!(float_eq!(actual, expected, abs <= 0.000_1));

        // Mid
        let uac_vol = UacVolume::new(UacVolume::DB_MIN, UacVolume::DB_MAX, 0).unwrap();
        let actual = uac_vol.normalized_to_db(0.5);
        let expected = UacVolume::to_db(0);
        assert!(float_eq!(actual, expected, abs <= 0.000_1));
    }

    #[test]
    fn uac_vol_min_out_of_range_err() {
        let too_low = i16::MIN;
        assert!(UacVolume::new(too_low, UacVolume::DB_MAX, 0).is_err());

        let max = 0x6555;
        let too_high = max + 1;
        assert!(UacVolume::new(too_high, max, 0).is_err());
    }

    #[test]
    fn uac_vol_max_out_of_range_err() {
        assert!(UacVolume::new(UacVolume::DB_MIN, UacVolume::DB_SILENCE, 0).is_err());
    }

    #[test]
    fn uac_vol_cur_out_of_range_err() {
        let min = UacVolume::DB_MIN + 10;
        let max = UacVolume::DB_MAX - 10;
        let too_low = min - 1;
        let too_high = max + 1;
        assert!(UacVolume::new(min, max, too_low).is_err());
        assert!(UacVolume::new(min, max, too_high).is_err());
    }

    #[test]
    fn uac_vol_good_values_are_ok() {
        if let Err(e) = UacVolume::new(UacVolume::DB_MIN, UacVolume::DB_MAX, 0) {
            println!("Error: {:?}", e);
            assert!(false);
        }
    }
}
