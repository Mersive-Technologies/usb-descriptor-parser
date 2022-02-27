#![allow(dead_code)] // TODO: tests around all code

use std::io::Write;

use anyhow::Error;
use crate::usb_proto::UsbDescriptorTypes;
use structure::byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};
use uuid::Uuid;

// UVC
// https://www.spinelelectronics.com/pdf/UVC%201.5%20Class%20specification.pdf
// https://github.com/torvalds/linux/blob/master/include/uapi/linux/usb/video.h

#[derive(FromPrimitive)]
#[repr(u8)]
pub enum UvcInterfaceSubClass {
    Undefined = 0x00,
    VideoControl = 0x01,
    VideoStreaming = 0x02,
    VideoInterfaceCollection = 0x03,
}

#[derive(FromPrimitive)]
#[repr(u8)]
pub enum UvcVsDescriptorSubtypes {
    Undefined = 0x00,
    InputHeader = 0x01,
    OutputHeader = 0x02,
    StillImageFrame = 0x03,
    FormatUncompressed = 0x04,
    FrameUncompressed = 0x05,
    FormatMjpeg = 0x06,
    FrameMjpeg = 0x07,
    FormatMpeg2ts = 0x0a,
    FormatDv = 0x0c,
    ColorFormat = 0x0d,
    FormatFrameBased = 0x10,
    FrameFrameBased = 0x11,
    FormatStreamBased = 0x12,
}

#[derive(FromPrimitive)]
#[repr(u8)]
pub enum UvcVcDescriptorSubtypes {
    UvcVcDescriptorUndefined = 0x00,
    UvcVcHeader = 0x01,
    UvcVcInputTerminal = 0x02,
    UvcVcOutputTerminal = 0x03,
    UvcVcSelectorUnit = 0x04,
    UvcVcProcessingUnit = 0x05,
    UvcVcExtensionUnit = 0x06,
}

#[derive(FromPrimitive)]
#[repr(u8)]
pub enum UvcRequestCodes {
    Undefined = 0x00,
    SetCur = 0x01,
    GetCur = 0x81,
    GetMin = 0x82,
    GetMax = 0x83,
    GetRes = 0x84,
    GetLen = 0x85,
    GetInfo = 0x86,
    GetDef = 0x87,
}

#[derive(FromPrimitive)]
#[repr(u8)]
pub enum UvcRequestTypes {
    ControlUndefined = 0x00,
    ProbeControl = 0x01,
    CommitControl = 0x02,
    StillProbeControl = 0x03,
    StillCommitControl = 0x04,
    StillImageTriggerControl = 0x05,
    StreamErrorCodeControl = 0x06,
    GenerateKeyFrameControl = 0x07,
    UpdateFrameSegmentControl = 0x08,
    SyncDelayControl = 0x09,
}

#[derive(FromPrimitive, Debug)]
#[repr(u8)]
pub enum UvcStreamErrorCodes {
    NoError = 0x00,
    ProtectedContent = 0x01,
    InputBufferUnderrun = 0x02,
    DataDiscontinuity = 0x03,
    OutputBufferUnderrun = 0x04,
    OutputBufferOverrun = 0x05,
    FormatChange = 0x06,
    StillImageCapture = 0x07,
}

pub struct UvcFrameHeader {
    pub b_header_length: u8,
    pub bm_header_info: u8,
    pub dw_presentation_time: u32,
    pub scr_source_clock: u64,
}

impl UvcFrameHeader {
    pub fn new(eof: bool, frame_id: bool) -> UvcFrameHeader {
        let flags: u8 = if eof { 1 << 1 } else { 0 }
            | if frame_id { 1 } else { 0 }
            | 0x80;
        UvcFrameHeader {
            b_header_length: UvcFrameHeader::size() as u8,
            bm_header_info: flags,
            dw_presentation_time: 0,
            scr_source_clock: 0,
        }
    }

    pub fn size() -> usize {
        let format = structure!("<BBIHI");
        return format.size();
    }

    pub fn deserialize(mut buffer: &mut &[u8]) -> Result<UvcFrameHeader, Error> {
        let format = structure!("<BBIHI");
        let (b_header_length, bm_header_info, dw_presentation_time, clk1, clk2) = format.unpack_from(&mut buffer)?;
        let scr_source_clock = (clk1 as u64) << 32 | clk2 as u64; // TODO: correct endianess?
        let msg = UvcFrameHeader { b_header_length, bm_header_info, dw_presentation_time, scr_source_clock };
        return Ok(msg);
    }

    pub fn serialize(&self, mut buffer: impl Write) -> Result<(), Error> {
        let format = structure!("<BBIHI");
        let clk1 = (self.scr_source_clock >> 32) as u16; // TODO: correct endianess?
        let clk2 = self.scr_source_clock as u32;
        format.pack_into(&mut buffer, self.b_header_length, self.bm_header_info, self.dw_presentation_time, clk1, clk2)?;
        return Ok(());
    }
}

#[derive(Debug, Clone)]
pub struct UvcOutputTerminalDescriptor {
    b_terminal_id: u8,
    w_terminal_type: u16,
    b_assoc_terminal: u8,
    b_source_id: u8,
    i_terminal: u8,
}
impl UvcOutputTerminalDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBHBBB");
        format.pack_into(&mut buffer,
                         self.size() as u8,
                         UsbDescriptorTypes::CsInterface as u8,
                         UvcVcDescriptorSubtypes::UvcVcOutputTerminal as u8,
                         self.b_terminal_id, self.w_terminal_type,
                         self.b_assoc_terminal, self.b_source_id, self.i_terminal,
        ).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> UvcOutputTerminalDescriptor {
        let format = structure!("<BHBBB");
        let (b_terminal_id, w_terminal_type, b_assoc_terminal, b_source_id, i_terminal)
            = format.unpack_from(&mut buffer).unwrap();
        let msg = UvcOutputTerminalDescriptor {
            b_terminal_id, w_terminal_type, b_assoc_terminal, b_source_id, i_terminal
        };
        return msg;
    }
    pub fn size(&self) -> usize {
        structure!("<BBBBHBBB").size()
    }
}

#[derive(Debug, Clone)]
pub struct UvcExtensionUnitDescriptor {
    b_unit_id: u8,
    guid_extension_code: Uuid,
    b_num_controls: u8,
    b_nr_in_pins: u8,
    ba_source_id: Vec<u8>,
    b_control_size: u8,
    bm_controls: Vec<u8>,
    i_extension: u8,
}

impl UvcExtensionUnitDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBB16sBB");
        format.pack_into(&mut buffer,
                         self.size() as u8,
                         UsbDescriptorTypes::CsInterface as u8,
                         UvcVcDescriptorSubtypes::UvcVcExtensionUnit as u8,
                         self.b_unit_id, self.guid_extension_code.as_bytes(), self.b_num_controls, self.b_nr_in_pins,
        ).unwrap();
        buffer.write_all(&self.ba_source_id).unwrap();
        buffer.write_u8(self.b_control_size).unwrap();
        buffer.write_all(&self.bm_controls).unwrap();
        buffer.write_u8(self.i_extension).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> UvcExtensionUnitDescriptor {
        let format = structure!("<B16sBB");
        let (b_unit_id, guid_extension_code, b_num_controls, b_nr_in_pins) = format.unpack_from(&mut buffer).unwrap();
        let mut guid_format = [0u8; 16];
        guid_format.copy_from_slice(&guid_extension_code[..]);
        let guid_extension_code = Uuid::from_bytes(guid_format);
        let ba_source_id = (0..b_nr_in_pins).map(|_| buffer.read_u8().unwrap()).collect();
        let b_control_size = buffer.read_u8().unwrap();
        let bm_controls = (0..b_control_size).map(|_| buffer.read_u8().unwrap()).collect();
        let i_extension = buffer.read_u8().unwrap();
        let msg = UvcExtensionUnitDescriptor {
            b_unit_id, guid_extension_code, b_num_controls, b_nr_in_pins, ba_source_id, b_control_size, bm_controls, i_extension
        };
        return msg;
    }
    pub fn size(&self) -> usize {
        structure!("<BBBB16sBB").size() + 2 + self.ba_source_id.len() + self.bm_controls.len()
    }
}

#[derive(Debug, Clone)]
pub struct UvcProcessingUnitDescriptor {
    b_unit_id: u8,
    b_source_id: u8,
    w_max_multiplier: u16,
    b_control_size: u8,
    bm_controls: u16,
    i_processing: u8,
    xtra: Vec<u8>,
}

impl UvcProcessingUnitDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBBHBHB");
        format.pack_into(&mut buffer,
                         self.size() as u8,
                         UsbDescriptorTypes::CsInterface as u8,
                         UvcVcDescriptorSubtypes::UvcVcProcessingUnit as u8,
                         self.b_unit_id, self.b_source_id, self.w_max_multiplier, self.b_control_size, self.bm_controls, self.i_processing,
        ).unwrap();
        buffer.write_all(&self.xtra).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8], len: u8) -> UvcProcessingUnitDescriptor {
        let format = structure!("<BBHBHB");
        let (b_unit_id, b_source_id, w_max_multiplier, b_control_size, bm_controls, i_processing) = format.unpack_from(&mut buffer).unwrap();
        let sz = len as usize - format.size() - 3;
        let xtra = (0..sz).map(|_| buffer.read_u8().unwrap()).collect();
        let msg = UvcProcessingUnitDescriptor {
            b_unit_id, b_source_id, w_max_multiplier, b_control_size, bm_controls, i_processing, xtra,
        };
        return msg;
    }
    pub fn size(&self) -> usize {
        structure!("<BBBBBHBHB").size() + self.xtra.len()
    }
}

#[derive(Debug, Clone)]
pub struct UvcInputTerminalDescriptor {
    b_terminal_id: u8,
    w_terminal_type: u16,
    b_assoc_terminal: u8,
    i_terminal: u8,
    xtra: Vec<u8>,
}

impl UvcInputTerminalDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBHBB");
        format.pack_into(&mut buffer,
                         self.size() as u8,
                         UsbDescriptorTypes::CsInterface as u8,
                         UvcVcDescriptorSubtypes::UvcVcInputTerminal as u8,
                         self.b_terminal_id, self.w_terminal_type, self.b_assoc_terminal, self.i_terminal,
        ).unwrap();
        buffer.write_all(&self.xtra).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8], len: u8) -> UvcInputTerminalDescriptor {
        let format = structure!("<BHBB");
        let (b_terminal_id, w_terminal_type, b_assoc_terminal, i_terminal) = format.unpack_from(&mut buffer).unwrap();
        let sz = len as usize - format.size() - 3;
        let xtra = (0..sz).map(|_| buffer.read_u8().unwrap()).collect();
        let msg = UvcInputTerminalDescriptor {
            b_terminal_id, w_terminal_type, b_assoc_terminal, i_terminal, xtra
        };
        return msg;
    }
    pub fn size(&self) -> usize {
        structure!("<BBBBHBB").size() + self.xtra.len()
    }
}

#[derive(Debug, Clone)]
pub struct UvcHeaderDescriptor {
    pub bcd_uvc: u16,
    pub w_total_length: u16,
    pub dw_clock_frequency: u32,
    pub b_in_collection: u8,
    pub ba_interface_nr: Vec<u8>,
}

impl UvcHeaderDescriptor {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBHHIB");
        format.pack_into(&mut buffer,
                         self.size() as u8,
                         UsbDescriptorTypes::CsInterface as u8,
                         UvcVcDescriptorSubtypes::UvcVcHeader as u8,
                         self.bcd_uvc, self.w_total_length, self.dw_clock_frequency, self.b_in_collection,
        ).unwrap();
        buffer.write_all(&self.ba_interface_nr).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> UvcHeaderDescriptor {
        let format = structure!("<HHIB");
        let (bcd_uvc, w_total_length, dw_clock_frequency, b_in_collection) = format.unpack_from(&mut buffer).unwrap();
        let ba_interface_nr = (0..b_in_collection).map(|_| buffer.read_u8().unwrap()).collect();
        let msg = UvcHeaderDescriptor {
            bcd_uvc, w_total_length, dw_clock_frequency, b_in_collection,
            ba_interface_nr
        };
        return msg;
    }
    pub fn size(&self) -> usize {
        structure!("<BBBHHIB").size() + self.ba_interface_nr.len()
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorUvcInputHeader {
    pub w_total_length: u16,
    pub b_endpoint_address: u8,
    pub bm_info: u8,
    pub b_terminal_link: u8,
    pub b_still_capture_method: u8,
    pub b_trigger_support: u8,
    pub b_trigger_usage: u8,
    pub b_control_size: u8,
    pub bma_controls: Vec<u8>,
}

impl DescriptorUvcInputHeader {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBHBBBBBBB");
        format.pack_into(&mut buffer, self.size() as u8, UsbDescriptorTypes::CsInterface as u8, UvcVsDescriptorSubtypes::InputHeader as u8, self.b_num_formats() as u8,
                         self.w_total_length, self.b_endpoint_address, self.bm_info, self.b_terminal_link, self.b_still_capture_method, self.b_trigger_support,
                         self.b_trigger_usage, self.b_control_size,
        ).unwrap();
        buffer.write_all(&self.bma_controls).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> DescriptorUvcInputHeader {
        let format = structure!("<BHBBBBBBB");
        let (
            b_num_formats, w_total_length, b_endpoint_address, bm_info, b_terminal_link, b_still_capture_method, b_trigger_support, b_trigger_usage, b_control_size
        ) = format.unpack_from(&mut buffer).unwrap();
        let sz = b_control_size * b_num_formats;
        let bma_controls = (0..sz).map(|_| buffer.read_u8().unwrap()).collect();
        let msg = DescriptorUvcInputHeader {
            w_total_length,
            b_endpoint_address,
            bm_info,
            b_terminal_link,
            b_still_capture_method,
            b_trigger_support,
            b_trigger_usage,
            b_control_size,
            bma_controls,
        };
        return msg;
    }
    pub fn b_num_formats(&self) -> usize {
        self.bma_controls.len() / self.b_control_size as usize
    }
    pub fn size(&self) -> usize {
        structure!("<BBBBHBBBBBBB").size() + self.bma_controls.len()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UvcStreamingControl {
    pub bm_hint: u16,
    pub b_format_index: u8,
    pub b_frame_index: u8,
    pub dw_frame_interval: u32,
    pub w_key_frame_rate: u16,
    pub w_pframe_rate: u16,
    pub w_comp_quality: u16,
    pub w_comp_window_size: u16,
    pub w_delay: u16,
    pub dw_max_video_frame_size: u32,
    pub dw_max_payload_transfer_size: u32,
}

impl UvcStreamingControl {
    pub fn deserialize(mut buffer: &mut &[u8]) -> UvcStreamingControl {
        let format = structure!("<HBBIHHHHHII");
        let (bm_hint, b_format_index, b_frame_index, dw_frame_interval, w_key_frame_rate, w_pframe_rate, w_comp_quality, w_comp_window_size, w_delay, dw_max_video_frame_size,
            dw_max_payload_transfer_size) = format.unpack_from(&mut buffer
        ).unwrap();
        let msg = UvcStreamingControl {
            bm_hint,
            b_format_index,
            b_frame_index,
            dw_frame_interval,
            w_key_frame_rate,
            w_pframe_rate,
            w_comp_quality,
            w_comp_window_size,
            w_delay,
            dw_max_video_frame_size,
            dw_max_payload_transfer_size,
        };
        return msg;
    }

    pub fn fps(&self) -> i32 {
        (1.0f32 / (self.dw_frame_interval as f32 / 10000000.0)).round() as i32
    }
}

#[non_exhaustive]
pub struct UncompressedFormats;

impl UncompressedFormats {
    pub const YUY2: Uuid = Uuid::from_bytes([0x32, 0x59, 0x55, 0x59, 0x00, 0x00, 0x00, 0x10, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71]);
    pub const NV12: Uuid = Uuid::from_bytes([0x32, 0x31, 0x56, 0x4E, 0x00, 0x00, 0x00, 0x10, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71]);
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorUvcFormatUncompressed {
    pub b_format_index: u8,
    pub b_num_frame_descriptors: u8,
    pub guid_format: Uuid,
    pub b_bits_per_pixel: u8,
    pub b_default_frame_index: u8,
    pub b_aspect_ratio_x: u8,
    pub b_aspect_ratio_y: u8,
    pub bm_interface_flags: u8,
    pub b_copy_protect: u8,
}

impl DescriptorUvcFormatUncompressed {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBBIHH8sBBBBBB");
        let (d1, d2, d3, d4) = self.guid_format.as_fields();
        format.pack_into(&mut buffer, format.size() as u8, UsbDescriptorTypes::CsInterface as u8, UvcVsDescriptorSubtypes::FormatUncompressed as u8,
            self.b_format_index, self.b_num_frame_descriptors, d1, d2, d3, d4, self.b_bits_per_pixel, self.b_default_frame_index, self.b_aspect_ratio_x,
            self.b_aspect_ratio_y, self.bm_interface_flags, self.b_copy_protect,
        ).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> DescriptorUvcFormatUncompressed {
        let format = structure!("<BBIHH8sBBBBBB");
        let (
            b_format_index, b_num_frame_descriptors, d1, d2, d3, d4, b_bits_per_pixel, b_default_frame_index, b_aspect_ratio_x, b_aspect_ratio_y, bm_interface_flags, b_copy_protect
        ) = format.unpack_from(&mut buffer).unwrap();
        let guid_format = Uuid::from_fields(d1, d2, d3, &d4[..]).unwrap();
        let msg = DescriptorUvcFormatUncompressed {
            b_format_index,
            b_num_frame_descriptors,
            guid_format,
            b_bits_per_pixel,
            b_default_frame_index,
            b_aspect_ratio_x,
            b_aspect_ratio_y,
            bm_interface_flags,
            b_copy_protect,
        };
        return msg;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorUvcFormatFrameBased {
    pub b_format_index: u8,
    pub b_num_frame_descriptors: u8,
    pub guid_format: Uuid,
    pub b_bits_per_pixel: u8,
    pub b_default_frame_index: u8,
    pub b_aspect_ratio_x: u8,
    pub b_aspect_ratio_y: u8,
    pub bm_interface_flags: u8,
    pub b_copy_protect: u8,
    pub b_variable_size: u8,
}

impl DescriptorUvcFormatFrameBased {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBBIHH8sBBBBBBB");
        let (d1, d2, d3, d4) = self.guid_format.as_fields();
        format.pack_into(&mut buffer, format.size() as u8, UsbDescriptorTypes::CsInterface as u8, UvcVsDescriptorSubtypes::FormatFrameBased as u8,
            self.b_format_index, self.b_num_frame_descriptors, d1, d2, d3, d4, self.b_bits_per_pixel, self.b_default_frame_index, self.b_aspect_ratio_x,
            self.b_aspect_ratio_y, self.bm_interface_flags, self.b_copy_protect, self.b_variable_size,
        ).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> DescriptorUvcFormatFrameBased {
        let format = structure!("<BBIHH8sBBBBBBB");
        let (
            b_format_index, b_num_frame_descriptors, d1, d2, d3, d4, b_bits_per_pixel, b_default_frame_index, b_aspect_ratio_x, b_aspect_ratio_y, bm_interface_flags, b_copy_protect, b_variable_size
        ) = format.unpack_from(&mut buffer).unwrap();
        let guid_format = Uuid::from_fields(d1, d2, d3, &d4[..]).unwrap();
        let msg = DescriptorUvcFormatFrameBased {
            b_format_index,
            b_num_frame_descriptors,
            guid_format,
            b_bits_per_pixel,
            b_default_frame_index,
            b_aspect_ratio_x,
            b_aspect_ratio_y,
            bm_interface_flags,
            b_copy_protect,
            b_variable_size,
        };
        return msg;
    }
}


#[derive(Debug, Clone, Copy)]
pub struct DescriptorUvcFormatMjpeg {
    pub b_format_index: u8,
    pub b_num_frame_descriptors: u8,
    pub bm_flags: u8,
    pub b_default_frame_index: u8,
    pub b_aspect_ratio_x: u8,
    pub b_aspect_ratio_y: u8,
    pub bm_interface_flags: u8,
    pub b_copy_protect: u8,
}

impl DescriptorUvcFormatMjpeg {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBBBBBBBB");
        format.pack_into(&mut buffer, format.size() as u8, UsbDescriptorTypes::CsInterface as u8, UvcVsDescriptorSubtypes::FormatMjpeg as u8, self.b_format_index, self.b_num_frame_descriptors, self.bm_flags,
                         self.b_default_frame_index, self.b_aspect_ratio_x, self.b_aspect_ratio_y, self.bm_interface_flags, self.b_copy_protect,
        ).unwrap();
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> DescriptorUvcFormatMjpeg {
        let format = structure!("<BBBBBBBB");
        let (
            b_format_index, b_num_frame_descriptors, bm_flags, b_default_frame_index, b_aspect_ratio_x, b_aspect_ratio_y, bm_interface_flags, b_copy_protect
        ) = format.unpack_from(&mut buffer).unwrap();
        let msg = DescriptorUvcFormatMjpeg { b_format_index, b_num_frame_descriptors, bm_flags, b_default_frame_index, b_aspect_ratio_x, b_aspect_ratio_y, bm_interface_flags, b_copy_protect };
        return msg;
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorUvcFrameUncompressed {
    pub b_frame_index: u8,
    pub bm_capabilities: u8,
    pub w_width: u16,
    pub w_height: u16,
    pub dw_min_bit_rate: u32,
    pub dw_max_bit_rate: u32,
    pub dw_max_video_frame_buffer_size: u32,
    pub dw_default_frame_interval: u32,
    pub dw_frame_interval: Vec<u32>,
}

impl DescriptorUvcFrameUncompressed {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBBHHIIIIB");
        let sz = format.size() as u8 + (self.b_frame_interval_type() * 4) as u8;
        format.pack_into(&mut buffer, sz, UsbDescriptorTypes::CsInterface as u8, UvcVsDescriptorSubtypes::FrameUncompressed as u8, self.b_frame_index, self.bm_capabilities, self.w_width,
                         self.w_height, self.dw_min_bit_rate, self.dw_max_bit_rate, self.dw_max_video_frame_buffer_size, self.dw_default_frame_interval, self.b_frame_interval_type() as u8,
        ).unwrap();
        self.dw_frame_interval.iter().for_each(|i| buffer.write_u32::<LittleEndian>(*i).unwrap());
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> DescriptorUvcFrameUncompressed {
        let format = structure!("<BBHHIIIIB");
        let (
            b_frame_index, bm_capabilities, w_width, w_height, dw_min_bit_rate, dw_max_bit_rate, dw_max_video_frame_buffer_size, dw_default_frame_interval, b_frame_interval_type
        ) = format.unpack_from(&mut buffer).unwrap();
        let dw_frame_interval = (0..b_frame_interval_type).map(|_| buffer.read_u32::<LittleEndian>().unwrap()).collect();
        let msg = DescriptorUvcFrameUncompressed {
            b_frame_index,
            bm_capabilities,
            w_width,
            w_height,
            dw_min_bit_rate,
            dw_max_bit_rate,
            dw_max_video_frame_buffer_size,
            dw_default_frame_interval,
            dw_frame_interval,
        };
        return msg;
    }
    pub fn b_frame_interval_type(&self) -> usize {
        self.dw_frame_interval.len()
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorUvcFrameMjpeg {
    pub b_frame_index: u8,
    pub bm_capabilities: u8,
    pub w_width: u16,
    pub w_height: u16,
    pub dw_min_bit_rate: u32,
    pub dw_max_bit_rate: u32,
    pub dw_max_video_frame_buffer_size: u32,
    pub dw_default_frame_interval: u32,
    pub dw_frame_interval: Vec<u32>,
}

impl DescriptorUvcFrameMjpeg {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBBHHIIIIB");
        format.pack_into(&mut buffer, self.size() as u8, UsbDescriptorTypes::CsInterface as u8, UvcVsDescriptorSubtypes::FrameMjpeg as u8, self.b_frame_index, self.bm_capabilities, self.w_width,
                         self.w_height, self.dw_min_bit_rate, self.dw_max_bit_rate, self.dw_max_video_frame_buffer_size, self.dw_default_frame_interval, self.b_frame_interval_type() as u8,
        ).unwrap();
        self.dw_frame_interval.iter().for_each(|i| buffer.write_u32::<LittleEndian>(*i).unwrap());
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> DescriptorUvcFrameMjpeg {
        let format = structure!("<BBHHIIIIB");
        let (
            b_frame_index, bm_capabilities, w_width, w_height, dw_min_bit_rate, dw_max_bit_rate, dw_max_video_frame_buffer_size, dw_default_frame_interval, b_frame_interval_type
        ) = format.unpack_from(&mut buffer).unwrap();
        let dw_frame_interval = (0..b_frame_interval_type).map(|_| buffer.read_u32::<LittleEndian>().unwrap()).collect();
        let msg = DescriptorUvcFrameMjpeg {
            b_frame_index,
            bm_capabilities,
            w_width,
            w_height,
            dw_min_bit_rate,
            dw_max_bit_rate,
            dw_max_video_frame_buffer_size,
            dw_default_frame_interval,
            dw_frame_interval,
        };
        return msg;
    }
    pub fn b_frame_interval_type(&self) -> usize {
        self.dw_frame_interval.len()
    }
    pub fn size(&self) -> usize {
        let format = structure!("<BBBBBHHIIIIB");
        format.size() + self.dw_frame_interval.len() * std::mem::size_of::<u32>()
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorUvcFrameFrameBased {
    pub b_frame_index: u8,
    pub bm_capabilities: u8,
    pub w_width: u16,
    pub w_height: u16,
    pub dw_min_bit_rate: u32,
    pub dw_max_bit_rate: u32,
    pub dw_default_frame_interval: u32,
    pub dw_bytes_per_line: u32,
    pub dw_frame_interval: Vec<u32>,
}

impl DescriptorUvcFrameFrameBased {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBBBBHHIIIBI");
        let sz = format.size() as u8 + (self.b_frame_interval_type() * 4) as u8;
        format.pack_into(&mut buffer, sz, UsbDescriptorTypes::CsInterface as u8, UvcVsDescriptorSubtypes::FrameFrameBased as u8, self.b_frame_index, self.bm_capabilities, self.w_width,
                         self.w_height, self.dw_min_bit_rate, self.dw_max_bit_rate, self.dw_default_frame_interval, self.b_frame_interval_type() as u8, self.dw_bytes_per_line,
        ).unwrap();
        self.dw_frame_interval.iter().for_each(|i| buffer.write_u32::<LittleEndian>(*i).unwrap());
    }
    pub fn deserialize(mut buffer: &mut &[u8]) -> DescriptorUvcFrameFrameBased {
        let format = structure!("<BBHHIIIBI");
        let (
            b_frame_index, bm_capabilities, w_width, w_height, dw_min_bit_rate, dw_max_bit_rate, dw_default_frame_interval, b_frame_interval_type, dw_bytes_per_line,
        ) = format.unpack_from(&mut buffer).unwrap();
        let dw_frame_interval = (0..b_frame_interval_type).map(|_| buffer.read_u32::<LittleEndian>().unwrap()).collect();
        let msg = DescriptorUvcFrameFrameBased {
            b_frame_index,
            bm_capabilities,
            w_width,
            w_height,
            dw_min_bit_rate,
            dw_max_bit_rate,
            dw_default_frame_interval,
            dw_frame_interval,
            dw_bytes_per_line,
        };
        return msg;
    }
    pub fn b_frame_interval_type(&self) -> usize {
        self.dw_frame_interval.len()
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorUvcVsInterfaceUnknown {
    pub iface_subclass: u8,
    pub bytes: Vec<u8>,
}

impl DescriptorUvcVsInterfaceUnknown {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBB");
        format.pack_into(&mut buffer, self.bytes.len() as u8 + 3u8, UsbDescriptorTypes::CsInterface as u8, self.iface_subclass).unwrap();
        buffer.write_all(&self.bytes).unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorUvcVcInterfaceUnknown {
    pub iface_subclass: u8,
    pub bytes: Vec<u8>,
}

impl DescriptorUvcVcInterfaceUnknown {
    pub fn serialize(&self, mut buffer: impl Write) {
        let format = structure!("<BBB");
        format.pack_into(&mut buffer, self.bytes.len() as u8 + 3u8, UsbDescriptorTypes::CsInterface as u8, self.iface_subclass).unwrap();
        buffer.write_all(&self.bytes).unwrap();
    }
}

#[derive(Debug,Copy,Clone)]
pub enum MockVideoFormat {
    Yuy2,
    Mjpeg,
    Nv12,
}

#[derive(Debug,Copy,Clone)]
pub struct MockVideoConfig {
    pub width: u32,
    pub height: u32,
    pub fps: i32,
    pub format: MockVideoFormat,
}

impl MockVideoConfig {
    pub fn new(width: u32, height: u32, fps: i32, format: MockVideoFormat) -> MockVideoConfig {
        MockVideoConfig { width, height, fps, format }
    }
}
